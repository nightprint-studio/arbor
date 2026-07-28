//! `core::persist` — encoding-aware read/write flush shared by the F12
//! (rename) and F13 (bulk-edit) project-wide orchestrators.
//!
//! Lifted verbatim from the 5 `*_studio` backends' `read_file_to_string`
//! / `write_to_disk` helpers, which were byte-identical: every format
//! decoded with [`arbor_fs::prelude::encoding::decode_bytes_full`] and
//! re-encoded with `encode_for_disk_with_bom`, preserving the file's
//! original encoding label + BOM (FROZEN F16 — per-file encoding
//! preservation across a multi-file refactor).
//!
//! Pure FS glue; no parsing, no format coupling. The orchestrators in
//! [`crate::refactor`] route every disk touch through here so the
//! encoding round-trip lives in exactly one place.

use arbor_studio_types::prelude::{StudioError, StudioResult};

/// A file read off disk together with the metadata needed to write it
/// back losslessly: the decoded text, the original encoding label, and
/// whether it carried a BOM.
#[derive(Debug, Clone)]
pub struct DecodedFile {
    pub text:           String,
    /// Encoding label (e.g. `"UTF-8"`, `"windows-1252"`) for the flush.
    pub encoding_label: String,
    pub had_bom:        bool,
}

/// Read + decode a file, reporting the encoding + BOM for a faithful
/// round-trip on write. Errors map to [`StudioError::App`] with the same
/// `Read {path}: {e}` shape the backends used.
pub fn read_decoded(abs_path: &str) -> StudioResult<DecodedFile> {
    let bytes = std::fs::read(abs_path)
        .map_err(|e| StudioError::App(format!("Read {abs_path}: {e}")))?;
    Ok(decode_for_edit(&bytes))
}

/// Decode bytes for editing. Pure — the half every backend needs, whether the
/// bytes came from disk, from a stream, or from a test.
///
/// The one thing it does beyond `decode_bytes_full` is **remove the byte-order
/// mark from the text**. That decoder leaves the BOM in the string as a leading
/// U+FEFF *and* reports `had_bom`, so writing the result back re-prepended a
/// second one and every save grew the file by three bytes, compounding. The BOM
/// is a property of the file, not a character of its content: it is stripped here
/// and re-added on the way out from `had_bom` alone.
///
/// Every backend must go through this rather than calling the decoder directly —
/// a leading U+FEFF also makes a strict parser reject an otherwise valid document.
pub fn decode_for_edit(bytes: &[u8]) -> DecodedFile {
    let (text, enc, had_bom) = arbor_fs::prelude::encoding::decode_bytes_full(bytes);
    DecodedFile {
        text: strip_leading_bom(text, had_bom),
        encoding_label: enc.name().to_string(),
        had_bom,
    }
}

/// Remove the byte-order mark the decoder left in the text.
///
/// Guarded by `had_bom` so a U+FEFF that genuinely belongs to the content — a
/// zero-width no-break space in the middle of a document that happens to start
/// with one — is only stripped when the file actually began with a mark.
fn strip_leading_bom(text: String, had_bom: bool) -> String {
    body_without_bom(&text, had_bom).to_string()
}

/// The document body, guaranteed free of a leading byte-order mark.
///
/// **Every writer must pass its content through this.** The BOM is written from
/// the `had_bom` flag alone; if it is *also* still present in the text — because
/// the content came from a reader that left it there — the file ends up with two,
/// and the next save makes three. Applying the rule at the write site rather than
/// trusting each reader means the bytes on disk are right regardless of which path
/// produced the string.
pub fn body_without_bom(contents: &str, had_bom: bool) -> &str {
    if had_bom {
        if let Some(rest) = contents.strip_prefix('\u{feff}') {
            return rest;
        }
    }
    contents
}

/// Best-effort decode: returns an empty string on any read failure.
/// Mirrors the backends' `read_file_to_string` (used for non-fatal
/// preview-line synthesis where a missing file just yields no snippet).
pub fn read_to_string_lossy(abs_path: &str) -> String {
    match std::fs::read(abs_path) {
        Ok(bytes) => arbor_fs::prelude::encoding::decode_bytes_full(&bytes).0,
        Err(_)    => String::new(),
    }
}

/// Re-encode `contents` with the original encoding + BOM and write it to
/// `path`, creating the parent directory if needed. Identical to the
/// per-format `write_to_disk` helpers.
pub fn write_encoded(
    path:           &str,
    contents:       &str,
    encoding_label: &str,
    had_bom:        bool,
) -> StudioResult<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StudioError::App(format!("mkdir {parent:?}: {e}")))?;
        }
    }
    // Strict, not the lossy sibling, for two reasons that are both data loss:
    // `encoding_rs` has no UTF-16 *encoder* and quietly redirects to UTF-8, so the
    // lossy path wrote UTF-8 bytes under a UTF-16 BOM — a corrupt file; and a
    // character the target encoding cannot hold was substituted with an HTML
    // numeric reference, so a pasted `日本語` became `&#x65E5;…` in a
    // windows-1252 file with nothing said about it. Refusing is the only honest
    // answer: the user still has their text, and can decide.
    let bytes = arbor_fs::prelude::encoding::encode_for_disk_strict(
        body_without_bom(contents, had_bom),
        Some(encoding_label),
        had_bom,
    )
    .map_err(|e| StudioError::App(format!("write {path}: {e}")))?;
    std::fs::write(path, &bytes)
        .map_err(|e| StudioError::App(format!("write {path}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("studio-persist-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// Read a file and write it straight back. Nothing about the file changed, so
    /// nothing about its bytes may change either.
    fn round_trip(name: &str, original: &[u8]) -> Vec<u8> {
        let dir = scratch(name);
        let path = dir.join("doc.json");
        let path = path.to_str().unwrap();
        std::fs::write(path, original).unwrap();

        let decoded = read_decoded(path).expect("read");
        write_encoded(path, &decoded.text, &decoded.encoding_label, decoded.had_bom).expect("write");

        let out = std::fs::read(path).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        out
    }

    #[test]
    fn a_utf8_bom_survives_a_save_exactly_once() {
        // The BOM must not accumulate. It did: the decoder left it in the string
        // as a leading U+FEFF *and* the writer prepended a fresh one, so every
        // save added another three bytes to the front of the file.
        let mut original = vec![0xEF, 0xBB, 0xBF];
        original.extend_from_slice(br#"{"a": 1}"#);
        assert_eq!(round_trip("bom-once", &original), original);
    }

    #[test]
    fn saving_twice_changes_nothing_the_second_time() {
        let dir = scratch("idempotent");
        let path = dir.join("doc.json");
        let path = path.to_str().unwrap();
        let mut original = vec![0xEF, 0xBB, 0xBF];
        original.extend_from_slice(br#"{"a": 1}"#);
        std::fs::write(path, &original).unwrap();

        for _ in 0..3 {
            let decoded = read_decoded(path).expect("read");
            write_encoded(path, &decoded.text, &decoded.encoding_label, decoded.had_bom)
                .expect("write");
        }
        assert_eq!(std::fs::read(path).unwrap(), original);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_utf16_file_is_still_utf16_after_a_save() {
        // `encoding_rs` has no UTF-16 encoder and quietly redirects to UTF-8, so
        // the old writer produced UTF-8 bytes under a UTF-16 BOM: a corrupt file.
        let text = "{\"a\": 1}";
        let mut original = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            original.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(round_trip("utf16", &original), original);
    }

    #[test]
    fn a_windows_1252_file_keeps_its_accents_as_single_bytes() {
        // `à` is a single 0xE0 byte in windows-1252 and two bytes in UTF-8, so a
        // save that silently switched encodings would show up here. Written as
        // literal bytes to keep this crate free of an encoding dependency.
        let original = b"// citt\xE0\n{\"a\": 1}".to_vec();
        assert_eq!(round_trip("cp1252", &original), original);
    }

    #[test]
    fn a_plain_utf8_file_without_a_bom_does_not_gain_one() {
        let original = "{\"a\": 1}".as_bytes().to_vec();
        assert_eq!(round_trip("no-bom", &original), original);
    }

    #[test]
    fn a_character_the_encoding_cannot_hold_fails_instead_of_mangling_the_file() {
        // Previously `encoding_rs` substituted an HTML numeric reference, so a
        // Japanese comment pasted into a windows-1252 file was silently written as
        // `&#x65E5;` — valid bytes, wrong content, no warning.
        let dir = scratch("unrepresentable");
        let path = dir.join("doc.json");
        let path = path.to_str().unwrap();
        let err = write_encoded(path, "// 日本語", "windows-1252", false)
            .expect_err("must refuse rather than substitute");
        let message = format!("{err:?}");
        assert!(message.contains('日'), "the message must name the character: {message}");
        assert!(!std::path::Path::new(path).exists(), "nothing may be written");
        let _ = std::fs::remove_dir_all(dir);
    }
}
