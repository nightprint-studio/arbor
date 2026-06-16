//! Recursive directory size (folder Properties) and multi-selection totals.

use std::path::Path;

use crate::entry::DirSize;

fn dir_size_blocking(path: &Path) -> DirSize {
    let mut acc = DirSize { bytes: 0, files: 0, dirs: 0 };
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => { acc.dirs += 1; stack.push(entry.path()); }
                Ok(meta) => { acc.files += 1; acc.bytes += meta.len(); }
                Err(_) => {}
            }
        }
    }
    acc
}

/// Recursively compute the size (bytes + file/dir counts) under `path`.
pub fn dir_size(path: &str) -> DirSize {
    dir_size_blocking(Path::new(path))
}

/// Total size of several paths at once (folders recursed, files summed) — the
/// multi-selection footer's "N items · X total" figure.
pub fn paths_size(paths: &[String]) -> DirSize {
    let mut acc = DirSize { bytes: 0, files: 0, dirs: 0 };
    for p in paths {
        let path = Path::new(p);
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => {
                acc.dirs += 1;
                let sub = dir_size_blocking(path);
                acc.bytes += sub.bytes; acc.files += sub.files; acc.dirs += sub.dirs;
            }
            Ok(meta) => { acc.files += 1; acc.bytes += meta.len(); }
            Err(_) => {}
        }
    }
    acc
}
