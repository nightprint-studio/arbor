//! Turning a downloaded zip into a folder: `arbor-fs`'s hardened extractor with this wizard's caps.
//!
//! The extractor refuses hostile names and symlinks outright and enforces the size caps while
//! inflating (see `arbor_fs::unzip_limited`); this module only picks the numbers and words the
//! error for a human. A project skeleton is a few hundred small files, so the caps are generous for
//! that and far below anything that could hurt.

use std::path::Path;

use arbor_fs::prelude::{unzip_limited, UnzipLimits, UnzipReport};

/// Caps for a downloaded skeleton.
pub const LIMITS: UnzipLimits = UnzipLimits {
    max_entries: 4_000,
    max_entry_bytes: 16 * 1024 * 1024,
    max_total_bytes: 96 * 1024 * 1024,
};

/// Extract `archive` into `dest`, stripping a single shared top folder if there is one.
pub fn extract_skeleton(archive: &Path, dest: &Path) -> Result<UnzipReport, String> {
    unzip_limited(archive, dest, &LIMITS, true).map_err(|e| format!("The downloaded archive was rejected: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_are_ordered_sanely() {
        assert!(LIMITS.max_entry_bytes <= LIMITS.max_total_bytes);
        assert!(LIMITS.max_total_bytes <= super::super::http::MAX_DOWNLOAD_BYTES * 2);
    }

    #[test]
    fn a_non_zip_is_rejected_with_a_sentence() {
        let d = std::env::temp_dir().join(format!("arbor-np-extract-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let bogus = d.join("x.zip");
        std::fs::write(&bogus, b"<html>an error page</html>").unwrap();
        let err = extract_skeleton(&bogus, &d.join("out")).unwrap_err();
        assert!(err.starts_with("The downloaded archive was rejected"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
