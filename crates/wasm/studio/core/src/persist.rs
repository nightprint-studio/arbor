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
    let (text, enc, had_bom) = arbor_fs::prelude::encoding::decode_bytes_full(&bytes);
    Ok(DecodedFile {
        text,
        encoding_label: enc.name().to_string(),
        had_bom,
    })
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
    let bytes = arbor_fs::prelude::encoding::encode_for_disk_with_bom(
        contents,
        Some(encoding_label),
        had_bom,
    );
    std::fs::write(path, &bytes)
        .map_err(|e| StudioError::App(format!("write {path}: {e}")))
}
