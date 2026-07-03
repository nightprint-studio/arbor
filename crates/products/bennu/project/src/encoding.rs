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
//! Phase-0 note: only UTF-8 and Cp1252 (Windows-1252) are *decoded* natively here —
//! Cp1252 is the legacy target stack's encoding and UTF-8 is the modern default, so
//! together they cover the Phase-0 corpus. The resolver still *reports* whatever
//! label was declared, so the FE always shows the true encoding; an unsupported label
//! decodes via a lossy UTF-8 fall-through with the true label preserved. A full
//! encoding matrix is a later dep decision (hard rule 7 — no encoding crate on the
//! approved list yet).

use crate::pom::Pom;

/// Resolve the encoding *label* Bennu will decode a project's files in: the pom's
/// `project.build.sourceEncoding`, else `default_label`.
pub fn project_encoding_label(pom: &Pom, default_label: &str) -> String {
    pom.property("project.build.sourceEncoding")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_label.to_string())
}

/// Decode `bytes` using `label`. Returns the decoded UTF-8 text plus the label that
/// actually applied. Native paths: `UTF-8` (with BOM strip) and `Cp1252` /
/// `Windows-1252`. Any other label falls through to lossy UTF-8 while preserving the
/// declared label (so the FE still shows e.g. `ISO-8859-1` even though Phase 0 didn't
/// transcode it) — a non-fatal degrade, not an error.
pub fn decode(bytes: &[u8], label: &str) -> (String, String) {
    let norm = label.to_ascii_lowercase().replace(['-', '_'], "");
    match norm.as_str() {
        "cp1252" | "windows1252" | "1252" => (decode_cp1252(bytes), label.to_string()),
        // UTF-8 (default) + anything unrecognised → UTF-8 (lossy), true label kept.
        _ => (decode_utf8(bytes), label.to_string()),
    }
}

/// Decode UTF-8, stripping a leading BOM. Invalid sequences become U+FFFD (lossy),
/// so a mislabelled file never hard-fails the open.
fn decode_utf8(bytes: &[u8]) -> String {
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(body).into_owned()
}

/// Decode Windows-1252 (Cp1252). Bytes 0x00–0x7F are ASCII; 0xA0–0xFF map to the
/// matching Latin-1 code points; 0x80–0x9F use the Windows-1252 punctuation block
/// (with the five undefined slots passed through as the raw code point). Pure table
/// lookup — no dependency (hard rule 7).
fn decode_cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| cp1252_char(b)).collect()
}

/// Map one Cp1252 byte to its Unicode scalar.
fn cp1252_char(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}', // €
        0x82 => '\u{201A}',
        0x83 => '\u{0192}',
        0x84 => '\u{201E}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02C6}',
        0x89 => '\u{2030}',
        0x8A => '\u{0160}',
        0x8B => '\u{2039}',
        0x8C => '\u{0152}',
        0x8E => '\u{017D}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02DC}',
        0x99 => '\u{2122}',
        0x9A => '\u{0161}',
        0x9B => '\u{203A}',
        0x9C => '\u{0153}',
        0x9E => '\u{017E}',
        0x9F => '\u{0178}',
        // 0x81, 0x8D, 0x8F, 0x90, 0x9D are undefined → pass the raw code point
        // through (Latin-1 identity), matching common lenient decoders.
        other => other as char,
    }
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
/// table lookup — the inverse of [`cp1252_char`] (no dependency, hard rule 7).
fn encode_cp1252(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        out.push(cp1252_byte(ch)?);
    }
    Some(out)
}

/// Map one Unicode scalar back to its Cp1252 byte (the inverse of [`cp1252_char`]).
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
