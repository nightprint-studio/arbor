//! Encoding-aware decode/encode for file content surfaced to the UI.
//!
//! Rationale: legacy codebases (Java, PHP, .properties, …) on Windows often
//! ship in `windows-1252` (CP1252, Latin-1 superset). Naïve UTF-8 decoding
//! either fails outright (`std::fs::read_to_string`) or produces U+FFFD
//! garbage (`from_utf8_lossy`). With the helpers here we sniff the file's
//! actual encoding once, decode losslessly, and remember the label so that
//! a later write back can re-encode to the original byte representation.

use encoding_rs::Encoding;

/// Detect the encoding of a buffer by inspecting its leading bytes.
///
/// Strategy:
/// 1. UTF-8 / UTF-16 BOM → that encoding.
/// 2. Strict UTF-8 validation → UTF-8 (the modern default).
/// 3. Otherwise → `windows-1252`. This is the right fallback for legacy
///    Java / PHP / .properties files on Windows European systems, where
///    the system default code page is CP1252 (Latin-1 superset). It is
///    also a safe choice because windows-1252 maps every byte to a
///    distinct codepoint, so the round-trip is lossless.
pub fn detect(bytes: &[u8]) -> &'static Encoding {
    if let Some((enc, _)) = Encoding::for_bom(bytes) {
        return enc;
    }
    if std::str::from_utf8(bytes).is_ok() {
        return encoding_rs::UTF_8;
    }
    encoding_rs::WINDOWS_1252
}

/// Detect the encoding and decode in one go.
pub fn decode_bytes(bytes: &[u8]) -> (String, &'static Encoding) {
    let enc = detect(bytes);
    let (cow, _) = enc.decode_without_bom_handling(bytes);
    (cow.into_owned(), enc)
}

/// Decode a buffer using a specific encoding (no detection). For when
/// several blobs of a file (stage 1/2/3) must share the same encoding so
/// the three-way view stays consistent.
pub fn decode_with(bytes: &[u8], encoding: &'static Encoding) -> String {
    let (cow, _) = encoding.decode_without_bom_handling(bytes);
    cow.into_owned()
}

/// Re-encode a string back to its original byte representation before
/// writing to disk. Falls back to UTF-8 when the label is unknown or `None`,
/// so callers that don't care about legacy encodings keep the existing
/// behaviour.
pub fn encode_for_disk(content: &str, encoding: Option<&str>) -> Vec<u8> {
    let enc = encoding
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (cow, _, _) = enc.encode(content);
    cow.into_owned()
}

/// Resolve a label to an encoding, falling back to UTF-8.
pub fn encoding_for_label(label: &str) -> &'static Encoding {
    Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::UTF_8)
}

/// `true` when the buffer starts with a UTF-8, UTF-16 LE, or UTF-16 BE
/// byte-order mark. `decode_without_bom_handling` strips BOMs from the
/// decoded `String`, so callers that need lossless round-trips have to
/// remember whether the original file had one and re-prepend at save.
pub fn has_bom(bytes: &[u8]) -> bool {
    Encoding::for_bom(bytes).is_some()
}

/// Return the BOM byte sequence appropriate for an encoding, or `None`
/// when the encoding has no canonical BOM.
pub fn bom_for(encoding: &'static Encoding) -> Option<&'static [u8]> {
    match encoding.name() {
        "UTF-8"    => Some(&[0xEF, 0xBB, 0xBF]),
        "UTF-16BE" => Some(&[0xFE, 0xFF]),
        "UTF-16LE" => Some(&[0xFF, 0xFE]),
        _          => None,
    }
}

/// Detect + decode + report BOM presence in one call. Companion to
/// `encode_for_disk_with_bom` for lossless round-trip of files that
/// originally shipped with a BOM.
pub fn decode_bytes_full(bytes: &[u8]) -> (String, &'static Encoding, bool) {
    let had_bom = has_bom(bytes);
    let (s, enc) = decode_bytes(bytes);
    (s, enc, had_bom)
}

/// Re-encode and optionally prepend the BOM appropriate for the target
/// encoding. Pair with `decode_bytes_full` to round-trip BOM-bearing
/// files without corruption.
pub fn encode_for_disk_with_bom(
    content:     &str,
    encoding:    Option<&str>,
    prepend_bom: bool,
) -> Vec<u8> {
    let enc = encoding
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (cow, _, _) = enc.encode(content);
    if prepend_bom {
        if let Some(bom) = bom_for(enc) {
            let mut out = Vec::with_capacity(bom.len() + cow.len());
            out.extend_from_slice(bom);
            out.extend_from_slice(&cow);
            return out;
        }
    }
    cow.into_owned()
}
