//! Extracting an archive you do not trust: [`unzip_limited`].
//!
//! [`crate::zip::unzip`] is the explorer's "Extract here" — it skips an unsafe entry and keeps
//! going, and it will happily inflate whatever the archive claims. That is the right behaviour for
//! a file the user chose; it is the wrong one for a **download**. This variant treats the archive
//! as hostile:
//!
//! - an entry whose name escapes the destination (absolute, `..`, a drive letter) makes the whole
//!   extraction fail, instead of being silently dropped — a archive with one is not a project
//!   skeleton, it is an attack;
//! - a symlink entry fails it too (a link written first and a file written through it second is the
//!   classic way past a name check);
//! - the entry count, each entry's size and the running total are capped, and the caps are enforced
//!   **while inflating**, not from the sizes the archive declares — those are the attacker's
//!   numbers;
//! - optionally, a single top-level folder shared by every entry is stripped, because the
//!   generators answer with `my-app/…` where a caller wants the contents.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{FsError, Result};

/// What an extraction may consume. The defaults suit a project skeleton (a few hundred small
/// files), not an archive of arbitrary content.
#[derive(Debug, Clone, Copy)]
pub struct UnzipLimits {
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for UnzipLimits {
    fn default() -> Self {
        Self { max_entries: 5_000, max_entry_bytes: 32 * 1024 * 1024, max_total_bytes: 128 * 1024 * 1024 }
    }
}

/// What was written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnzipReport {
    pub files: usize,
    pub bytes: u64,
}

/// The components of an entry name, or why the name is not acceptable.
///
/// Both separators are accepted (some archivers write `\`), and empty and `.` components are
/// dropped. Everything that could point outside the destination is an error.
pub fn entry_components(name: &str) -> std::result::Result<Vec<String>, String> {
    if name.contains('\0') {
        return Err(format!("entry name contains a NUL byte: {name:?}"));
    }
    let unified = name.replace('\\', "/");
    if unified.starts_with('/') {
        return Err(format!("absolute entry name: {name}"));
    }
    let mut out = Vec::new();
    for part in unified.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(format!("entry escapes the destination: {name}")),
            p if p.len() >= 2 && p.as_bytes()[1] == b':' && p.as_bytes()[0].is_ascii_alphabetic() => {
                return Err(format!("entry names a drive: {name}"))
            }
            p => out.push(p.to_string()),
        }
    }
    Ok(out)
}

/// The single first component shared by every entry, when there is one *and* it is a folder (some
/// entry lies beneath it). A lone file at the root is not a root to strip.
pub fn shared_root(entries: &[Vec<String>]) -> Option<String> {
    let first = entries.iter().find(|c| !c.is_empty())?.first()?.clone();
    let all_share = entries.iter().filter(|c| !c.is_empty()).all(|c| c[0] == first);
    let is_folder = entries.iter().any(|c| c.len() > 1 && c[0] == first);
    (all_share && is_folder).then_some(first)
}

fn is_symlink(mode: Option<u32>) -> bool {
    mode.is_some_and(|m| m & 0o170000 == 0o120000)
}

/// Extract `archive` into `dest` (created if missing) under `limits`. See the module docs.
///
/// On any error some files may already be written; the caller extracts into a scratch directory
/// and deletes it on failure.
pub fn unzip_limited(
    archive: &Path,
    dest: &Path,
    limits: &UnzipLimits,
    strip_shared_root: bool,
) -> Result<UnzipReport> {
    let file = std::fs::File::open(archive).map_err(|e| FsError::io("Cannot open archive", e))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| FsError::Zip(format!("Not a valid ZIP archive: {e}")))?;
    if zip.len() > limits.max_entries {
        return Err(FsError::Zip(format!("Archive has too many entries ({})", zip.len())));
    }

    // First pass: names only, so a hostile entry anywhere fails the run before a byte is written.
    let mut names: Vec<Vec<String>> = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| FsError::Zip(format!("Cannot read entry: {e}")))?;
        if is_symlink(entry.unix_mode()) {
            return Err(FsError::Zip(format!("Archive contains a symbolic link: {}", entry.name())));
        }
        names.push(entry_components(entry.name()).map_err(FsError::Zip)?);
    }
    let root = if strip_shared_root { shared_root(&names) } else { None };

    std::fs::create_dir_all(dest).map_err(|e| FsError::io("Cannot create output folder", e))?;
    let mut report = UnzipReport::default();
    for (i, comps) in names.iter().enumerate() {
        let comps: &[String] = match &root {
            Some(_) => comps.get(1..).unwrap_or(&[]),
            None => comps,
        };
        if comps.is_empty() {
            continue;
        }
        let mut entry = zip.by_index(i).map_err(|e| FsError::Zip(format!("Cannot read entry: {e}")))?;
        let out: PathBuf = comps.iter().fold(dest.to_path_buf(), |p, c| p.join(c));
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| FsError::io("Cannot create dir", e))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| FsError::io("Cannot create dir", e))?;
        }
        let mut dst = std::fs::File::create(&out).map_err(|e| FsError::io("Cannot write file", e))?;
        // `take(limit + 1)`: reading one byte past the cap is how an over-size entry is noticed
        // without trusting its declared size.
        let cap = limits.max_entry_bytes.min(limits.max_total_bytes.saturating_sub(report.bytes));
        let written = std::io::copy(&mut (&mut entry).take(cap + 1), &mut dst)
            .map_err(|e| FsError::io("Cannot extract file", e))?;
        if written > cap {
            return Err(FsError::Zip(format!(
                "Archive is larger than allowed (while extracting {})",
                comps.join("/")
            )));
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o777));
        }
        report.files += 1;
        report.bytes += written;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn comps(s: &str) -> Vec<String> {
        entry_components(s).unwrap()
    }

    #[test]
    fn plain_names_are_split_and_normalised() {
        assert_eq!(comps("a/b/c.txt"), ["a", "b", "c.txt"]);
        assert_eq!(comps("a\\b\\c.txt"), ["a", "b", "c.txt"]);
        assert_eq!(comps("./a//b/"), ["a", "b"]);
    }

    #[test]
    fn names_that_escape_are_refused() {
        for bad in ["../x", "a/../../x", "/etc/passwd", "\\evil", "C:/x", "a/c:/x", "a\0b"] {
            assert!(entry_components(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn shared_root_needs_a_folder_everyone_is_under() {
        let e = |v: &[&str]| v.iter().map(|s| comps(s)).collect::<Vec<_>>();
        assert_eq!(shared_root(&e(&["app/", "app/pom.xml", "app/src/A.java"])), Some("app".into()));
        assert_eq!(shared_root(&e(&["app/pom.xml", "other/x"])), None);
        assert_eq!(shared_root(&e(&["pom.xml"])), None);
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("arbor-fs-unzip-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn extracts_and_strips_the_shared_root() {
        let d = scratch("ok");
        let z = d.join("a.zip");
        make_zip(&z, &[("app/pom.xml", b"<p/>"), ("app/src/A.java", b"class A{}")]);
        let out = d.join("out");
        let r = unzip_limited(&z, &out, &UnzipLimits::default(), true).unwrap();
        assert_eq!(r.files, 2);
        assert!(out.join("pom.xml").is_file() && out.join("src/A.java").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_traversal_entry_fails_the_whole_extraction_before_writing() {
        let d = scratch("slip");
        let z = d.join("a.zip");
        make_zip(&z, &[("good.txt", b"x"), ("../evil.txt", b"y")]);
        let out = d.join("out");
        assert!(unzip_limited(&z, &out, &UnzipLimits::default(), false).is_err());
        assert!(!out.join("good.txt").exists(), "nothing is written when a name is hostile");
        assert!(!d.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn size_caps_are_enforced_while_inflating() {
        let d = scratch("big");
        let z = d.join("a.zip");
        make_zip(&z, &[("big.bin", &vec![0u8; 4096])]);
        let tight = UnzipLimits { max_entries: 10, max_entry_bytes: 1024, max_total_bytes: 1 << 20 };
        assert!(unzip_limited(&z, &d.join("o1"), &tight, false).is_err());
        let few = UnzipLimits { max_entries: 0, ..UnzipLimits::default() };
        assert!(unzip_limited(&z, &d.join("o2"), &few, false).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
