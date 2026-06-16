//! Copy / move / duplicate with progress reporting and cooperative cancel.
//!
//! The pure layer drives the walk, scans the totals up front, reports each
//! copied file through a [`ProgressSink`], and bails out at the next file
//! boundary when the [`CancelToken`] is set. The shell supplies a sink that
//! throttles and emits the `arbor://fs-op-progress` Tauri event, and owns the
//! op-id → token registry that backs the cancel command.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{FsError, Result};

/// Receives copy/move progress. The shell's implementation throttles and emits;
/// arbor-fs only reports facts. `start` fires once before any file, `file_done`
/// after each copied file (cumulative counters), `finish` once at the end.
pub trait ProgressSink {
    fn start(&mut self, total_files: u64, total_bytes: u64);
    fn file_done(&mut self, done_files: u64, done_bytes: u64, current: &str);
    fn finish(&mut self, done_files: u64, done_bytes: u64);
}

/// A no-op sink — the fast path for small / scriptless calls with no progress.
pub struct NoopSink;
impl ProgressSink for NoopSink {
    fn start(&mut self, _: u64, _: u64) {}
    fn file_done(&mut self, _: u64, _: u64, _: &str) {}
    fn finish(&mut self, _: u64, _: u64) {}
}

/// A cooperative cancel flag, shared between the cancel command (sets it) and a
/// running copy/move/duplicate (polls it at each file boundary).
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Pick a non-colliding destination path inside `dir` for an entry named
/// `name`, appending " (2)", " (3)", … before the extension on collision —
/// mirroring Windows Explorer's paste behaviour.
pub(crate) fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let ext = p.extension().map(|e| e.to_string_lossy().to_string());
    let mut i = 2;
    loop {
        let candidate_name = match &ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        let candidate = dir.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}

/// Recursively count the files and total bytes under `src` so the progress bar
/// has a denominator. Symlinks are counted as files (not followed).
fn scan_totals(src: &Path, files: &mut u64, bytes: &mut u64) {
    match std::fs::symlink_metadata(src) {
        Ok(meta) if meta.is_dir() => {
            if let Ok(rd) = std::fs::read_dir(src) {
                for e in rd.flatten() {
                    scan_totals(&e.path(), files, bytes);
                }
            }
        }
        Ok(meta) => { *files += 1; *bytes += meta.len(); }
        Err(_) => {}
    }
}

/// Drives one copy/move/duplicate: holds the sink, the cancel token and the
/// running counters, and recurses copying files.
struct Walk<'a> {
    sink:       &'a mut dyn ProgressSink,
    cancel:     &'a CancelToken,
    done_files: u64,
    done_bytes: u64,
}

impl Walk<'_> {
    /// Progress- and cancel-aware recursive copy. Reports each copied file and
    /// bails out (with [`FsError::Cancelled`]) when the cancel flag is set.
    fn copy_tree(&mut self, src: &Path, dst: &Path) -> Result<()> {
        if self.cancel.is_cancelled() {
            return Err(FsError::Cancelled);
        }
        let meta = std::fs::symlink_metadata(src)
            .map_err(|e| FsError::io(format!("Cannot read {}", src.display()), e))?;
        if meta.is_dir() {
            std::fs::create_dir_all(dst)
                .map_err(|e| FsError::io(format!("Cannot create {}", dst.display()), e))?;
            for entry in std::fs::read_dir(src)
                .map_err(|e| FsError::io(format!("Cannot read {}", src.display()), e))?
            {
                let entry = entry.map_err(|e| FsError::io("Cannot read entry", e))?;
                self.copy_tree(&entry.path(), &dst.join(entry.file_name()))?;
            }
        } else {
            if let Some(parent) = dst.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::copy(src, dst)
                .map_err(|e| FsError::io(format!("Cannot copy {}", src.display()), e))?;
            self.done_files += 1;
            self.done_bytes += meta.len();
            let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            self.sink.file_done(self.done_files, self.done_bytes, &name);
        }
        Ok(())
    }
}

/// Pre-scan `sources` for the file/byte totals (the progress denominator).
fn totals(sources: &[String]) -> (u64, u64) {
    let (mut tf, mut tb) = (0u64, 0u64);
    for s in sources {
        scan_totals(Path::new(s), &mut tf, &mut tb);
    }
    (tf, tb)
}

/// Copy each of `sources` into `dest_dir`. With `overwrite = false` name
/// collisions are resolved Explorer-style (" (2)", " (3)", …); with
/// `overwrite = true` each item keeps its name and merges into any existing
/// folder of the same name, replacing colliding files (recursive). Returns the
/// list of created / merged destination paths.
pub fn copy(
    sources: &[String],
    dest_dir: &str,
    overwrite: bool,
    sink: &mut dyn ProgressSink,
    cancel: &CancelToken,
) -> Result<Vec<String>> {
    let (tf, tb) = totals(sources);
    sink.start(tf, tb);
    let mut walk = Walk { sink, cancel, done_files: 0, done_bytes: 0 };

    let dir = Path::new(dest_dir);
    let mut created = Vec::with_capacity(sources.len());
    for s in sources {
        let src = Path::new(s);
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| FsError::Invalid(format!("Invalid source path: {s}")))?;
        let dst = if overwrite { dir.join(&name) } else { unique_dest(dir, &name) };
        walk.copy_tree(src, &dst)?;
        created.push(dst.to_string_lossy().to_string());
    }
    walk.sink.finish(walk.done_files, walk.done_bytes);
    Ok(created)
}

/// Move each of `sources` into `dest_dir` (cut + paste). Falls back to
/// copy-then-delete across volumes where `rename` can't work. With
/// `overwrite = true` an item keeps its name and merges into / replaces an
/// existing same-named entry instead of getting a " (2)" suffix. Returns the
/// list of new paths. Moving into the same directory is a no-op.
pub fn move_(
    sources: &[String],
    dest_dir: &str,
    overwrite: bool,
    sink: &mut dyn ProgressSink,
    cancel: &CancelToken,
) -> Result<Vec<String>> {
    // Same-volume moves are instant renames (no per-file progress); only the
    // cross-volume copy+delete fallback reports per-file, but we still scan the
    // totals up front so the bar is sized for that case.
    let (tf, tb) = totals(sources);
    sink.start(tf, tb);
    let mut walk = Walk { sink, cancel, done_files: 0, done_bytes: 0 };

    let dir = Path::new(dest_dir);
    let mut moved = Vec::with_capacity(sources.len());
    for s in sources {
        if walk.cancel.is_cancelled() {
            return Err(FsError::Cancelled);
        }
        let src = Path::new(s);
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| FsError::Invalid(format!("Invalid source path: {s}")))?;
        // No-op when the destination is the source's own parent.
        if src.parent() == Some(dir) {
            moved.push(s.clone());
            continue;
        }
        // Refuse to move a directory into itself or a descendant.
        if dir.starts_with(src) {
            return Err(FsError::Invalid("Cannot move a folder into itself".into()));
        }
        let dst = if overwrite { dir.join(&name) } else { unique_dest(dir, &name) };
        if std::fs::rename(src, &dst).is_err() {
            walk.copy_tree(src, &dst)?;
            let removed = if src.is_dir() {
                std::fs::remove_dir_all(src)
            } else {
                std::fs::remove_file(src)
            };
            removed.map_err(|e| FsError::io(format!("Cannot remove source {name}"), e))?;
        }
        moved.push(dst.to_string_lossy().to_string());
    }
    walk.sink.finish(walk.done_files, walk.done_bytes);
    Ok(moved)
}

/// Duplicate each of `paths` in place (same parent folder), Explorer-style:
/// `report.pdf` → `report (2).pdf`, a second time → `report (3).pdf`. Returns
/// the created paths. Progress/cancel work exactly like [`copy`].
pub fn duplicate(
    paths: &[String],
    sink: &mut dyn ProgressSink,
    cancel: &CancelToken,
) -> Result<Vec<String>> {
    let (tf, tb) = totals(paths);
    sink.start(tf, tb);
    let mut walk = Walk { sink, cancel, done_files: 0, done_bytes: 0 };

    let mut created = Vec::with_capacity(paths.len());
    for s in paths {
        let src = Path::new(s);
        let parent = src.parent()
            .ok_or_else(|| FsError::Invalid(format!("Cannot duplicate a root path: {s}")))?;
        let name = src.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| FsError::Invalid(format!("Invalid source path: {s}")))?;
        let dst = unique_dest(parent, &name);
        walk.copy_tree(src, &dst)?;
        created.push(dst.to_string_lossy().to_string());
    }
    walk.sink.finish(walk.done_files, walk.done_bytes);
    Ok(created)
}
