//! The content store: bytes named by what they are.
//!
//! Content addressing is not an optimisation here, it is what makes the retention
//! policy honest. Saving a file ten times having changed it twice costs three blobs, so
//! the size budget is spent on distinct content rather than on how often somebody hits
//! ⌘S. It also means a revert is a lookup: there is no chain of diffs to replay, and no
//! way for a corrupt link in the middle to take the ones after it down with it.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::HistoryResult;

/// Hex sha-256 of `bytes` — a blob's name.
pub fn hash(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Short stable key for a string (a project root, a relative path). Sixteen hex
/// characters: enough that a collision is not a thing that happens to a project, short
/// enough that the store stays browsable by a human with a file manager.
pub fn key(text: &str) -> String {
    hash(text.as_bytes())[..16].to_string()
}

/// Where a blob lives under `root`. Fanned out one byte deep, because a project with a
/// year of history is tens of thousands of blobs and a single flat directory of those
/// is slow to list on every platform that matters.
pub fn blob_path(root: &Path, hash: &str) -> PathBuf {
    root.join("blobs").join(&hash[..2]).join(hash)
}

/// Write `bytes` and return their hash. A blob that is already there is left alone —
/// same name means same content, so rewriting it could only replace it with itself.
pub fn put(root: &Path, bytes: &[u8]) -> HistoryResult<String> {
    let h = hash(bytes);
    let path = blob_path(root, &h);
    if path.exists() {
        return Ok(h);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Write beside and rename: a blob is either wholly there under its own name or not
    // there at all, so a crash mid-write cannot leave a name promising content it does
    // not have.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(h)
}

/// Read a blob back.
pub fn get(root: &Path, hash: &str) -> HistoryResult<Vec<u8>> {
    Ok(std::fs::read(blob_path(root, hash))?)
}

/// Delete a blob, reporting the bytes freed.
pub fn remove(root: &Path, hash: &str) -> u64 {
    let path = blob_path(root, hash);
    let n = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if std::fs::remove_file(&path).is_ok() {
        n
    } else {
        0
    }
}

/// Every blob currently on disk.
pub fn all(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(shards) = std::fs::read_dir(root.join("blobs")) else { return out };
    for shard in shards.flatten() {
        let Ok(files) = std::fs::read_dir(shard.path()) else { continue };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            // Skip the `.tmp` of an interrupted write — it is not a blob yet, and
            // naming it as one would make the GC delete it while a writer holds it.
            if name.len() == 64 {
                out.push(name);
            }
        }
    }
    out
}

/// Total bytes the blobs occupy.
pub fn total_bytes(root: &Path) -> u64 {
    all(root).iter().map(|h| std::fs::metadata(blob_path(root, h)).map(|m| m.len()).unwrap_or(0)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_is_one_blob() {
        let dir = std::env::temp_dir().join(format!("arbor-hist-blob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let a = put(&dir, b"fn main() {}").unwrap();
        let b = put(&dir, b"fn main() {}").unwrap();
        assert_eq!(a, b);
        assert_eq!(all(&dir).len(), 1, "the same bytes twice cost one blob");
        assert_eq!(get(&dir, &a).unwrap(), b"fn main() {}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
