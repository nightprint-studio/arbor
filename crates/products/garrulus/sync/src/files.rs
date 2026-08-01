//! The filesystem primitives the seam owns: walking a vault, reading and
//! writing a note, and a content hash.
//!
//! Deliberately tiny and dependency-free. `arbor-fs` owns the *product's* file
//! I/O (trash, encoding, roots); what is here is the mechanical part the sync
//! engine needs to do without a vault loaded, so a unit test can drive it
//! against a temp directory.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::change::RelPath;

/// The one dot-folder a product may write into a project (CLAUDE.md, and
/// `docs/garrulus-design.md` §3.3).
pub const MARKER_DIR: &str = ".arbor";

/// Directories a vault walk never descends into.
///
/// Everything hidden is skipped, which covers `.git`, `.arbor` and whatever
/// else a sync client leaves lying around (`.obsidian`, `.Trash-1000`, …).
pub fn is_skipped_dir(name: &str) -> bool {
    name.starts_with('.')
}

/// Every markdown note in the vault, vault-relative and sorted.
///
/// Sorted so that two machines walking the same vault produce the same order —
/// a diff of two manifests is then a line diff, not a set operation.
pub fn walk_notes(root: &Path) -> io::Result<Vec<RelPath>> {
    let mut out = Vec::new();
    if root.is_dir() {
        walk_into(root, root, &mut out)?;
    }
    out.sort();
    Ok(out)
}

fn walk_into(root: &Path, dir: &Path, out: &mut Vec<RelPath>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if !is_skipped_dir(&name) {
                walk_into(root, &path, out)?;
            }
            continue;
        }
        if let Some(rel) = RelPath::from_abs(root, &path) {
            if rel.is_note() {
                out.push(rel);
            }
        }
    }
    Ok(())
}

/// Read a note, or `None` when it is not there.
///
/// Lossy on purpose: a note that is not valid UTF-8 must still be merge-able
/// rather than a hard error in the middle of a pull.
pub fn read_note(path: &Path) -> io::Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).to_string())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Write a note, creating the folders above it.
///
/// Writes through a sibling temp file and renames, so a crash mid-write leaves
/// the previous note intact rather than a truncated one — the vault is the
/// record, and a half-written record is worse than a stale one.
pub fn write_note(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("garrulus-tmp");
    fs::write(&tmp, text.as_bytes())?;
    fs::rename(&tmp, path)
}

/// FNV-1a 64 of the file's bytes, or `None` when it is not there.
///
/// Change detection only — never identity, never security. A note-sized file
/// hashes in microseconds and needs no dependency.
pub fn hash_file(path: &Path) -> io::Result<Option<u64>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(fnv1a64(&bytes))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// FNV-1a, 64 bit.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Hash every note under `root`, keyed by vault-relative path.
pub fn hash_tree(root: &Path) -> io::Result<BTreeMap<RelPath, u64>> {
    let mut out = BTreeMap::new();
    for rel in walk_notes(root)? {
        if let Some(h) = hash_file(&rel.to_path(root))? {
            out.insert(rel, h);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_directories_are_skipped() {
        assert!(is_skipped_dir(".git"));
        assert!(is_skipped_dir(".arbor"));
        assert!(is_skipped_dir(".obsidian"));
        assert!(!is_skipped_dir("bugs"));
    }

    #[test]
    fn fnv_is_stable_and_discriminating() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"nota"), fnv1a64(b"nota"));
        assert_ne!(fnv1a64(b"nota"), fnv1a64(b"notb"));
    }
}
