//! Mutating operations: create, rename (single + batch), write, delete.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{FsError, Result};

/// Create a directory and all missing parents.
pub fn create_dir(path: &str) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| FsError::io("Cannot create directory", e))
}

/// Create an empty file, making parent directories first if needed.
pub fn create_file(path: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::File::create(path)
        .map(|_| ())
        .map_err(|e| FsError::io("Cannot create file", e))
}

/// Rename / move a single path.
pub fn rename(old_path: &str, new_path: &str) -> Result<()> {
    std::fs::rename(old_path, new_path).map_err(|e| FsError::io("Cannot rename", e))
}

/// One old→new rename pair for a batch rename.
#[derive(Debug, Deserialize)]
pub struct RenamePair {
    pub from: String,
    pub to:   String,
}

/// Batch-rename in two phases so order-independent shuffles (e.g. `a→b, b→c`,
/// or swapping two names) don't clobber each other: every source is first moved
/// to a unique temp name, then to its final name. Validates up front that the
/// final names are unique and don't collide with files left untouched. All
/// targets share the parent of their source.
pub fn rename_many(pairs: &[RenamePair]) -> Result<Vec<String>> {
    use std::collections::HashSet;
    // Reject duplicate destinations early — two files can't take the same name.
    let mut seen = HashSet::new();
    for p in pairs {
        let to = Path::new(&p.to);
        let parent = to.parent().unwrap_or_else(|| Path::new(""));
        let key = parent.join(to.file_name().unwrap_or_default());
        if !seen.insert(key) {
            return Err(FsError::Invalid(format!(
                "Two items would be renamed to the same name: {}",
                to.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            )));
        }
    }
    // Reject a destination that already exists and isn't itself being renamed
    // away in this batch (so shifting a contiguous block is fine, overwriting an
    // unrelated file is not).
    let froms: HashSet<PathBuf> = pairs.iter().map(|p| PathBuf::from(&p.from)).collect();
    for p in pairs {
        let to = Path::new(&p.to);
        if to.exists() && !froms.contains(to) {
            return Err(FsError::Invalid(format!(
                "A file named '{}' already exists",
                to.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            )));
        }
    }
    // Phase 1: source → unique temp.
    let mut temps: Vec<(PathBuf, String)> = Vec::with_capacity(pairs.len());
    for (i, p) in pairs.iter().enumerate() {
        let from = Path::new(&p.from);
        let parent = from.parent().unwrap_or_else(|| Path::new("."));
        let tmp = parent.join(format!(".arbor-rename-{i}.tmp"));
        std::fs::rename(from, &tmp)
            .map_err(|e| FsError::io(format!("Cannot rename {}", p.from), e))?;
        temps.push((tmp, p.to.clone()));
    }
    // Phase 2: temp → final.
    let mut out = Vec::with_capacity(temps.len());
    for (tmp, to) in &temps {
        std::fs::rename(tmp, to)
            .map_err(|e| FsError::io(format!("Cannot rename to {to}"), e))?;
        out.push(to.clone());
    }
    Ok(out)
}

/// Write a text file, creating it (or overwriting it) at the given path.
/// Parent directories are created automatically if they don't exist.
pub fn write_text(path: &str, content: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, content.as_bytes()).map_err(|e| FsError::io("Cannot write file", e))
}

/// Delete a single file or directory (recursively for dirs).
pub fn delete(path: &str) -> Result<()> {
    let p = Path::new(path);
    let result = if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    };
    result.map_err(|e| FsError::io("Cannot delete", e))
}

/// Permanently delete several paths from disk (files or directories).
pub fn delete_many(paths: &[String]) -> Result<()> {
    for p in paths {
        let path = Path::new(p);
        let r = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        r.map_err(|e| FsError::io(format!("Cannot delete {p}"), e))?;
    }
    Ok(())
}
