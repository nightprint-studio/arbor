//! Trash / Recycle Bin: move-to-trash, restore, and the Recycle Bin view
//! (list / restore / purge / empty).
//!
//! Windows + Linux are backed by `trash::os_limited` (with original locations,
//! so restore is a true "Put Back"). macOS has no such API, so it's backed by
//! the user's `~/.Trash` directory directly: list reads it, purge/empty remove
//! from it, and restore recovers to the Desktop (the original path isn't stored
//! anywhere readable, so a real Put Back isn't possible there).

use crate::entry::TrashEntry;
use crate::error::{FsError, Result};

/// Move paths to the OS trash / Recycle Bin (recoverable).
pub fn trash(paths: &[String]) -> Result<()> {
    trash::delete_all(paths).map_err(|e| FsError::Trash(format!("Cannot move to trash: {e}")))
}

/// Restore previously-trashed entries back to their original locations — the
/// "undo" of [`trash`]. For each requested original path, the most recently
/// deleted matching item in the Recycle Bin is restored. Backed by the OS trash
/// index (Windows / Linux); not supported on macOS (no restore API).
pub fn untrash(paths: &[String]) -> Result<()> {
    untrash_paths(paths)
}

#[cfg(not(target_os = "macos"))]
fn untrash_paths(paths: &[String]) -> Result<()> {
    use std::path::Path;
    use trash::os_limited::{list, restore_all};

    let items = list().map_err(|e| FsError::Trash(format!("Cannot read the Recycle Bin: {e}")))?;
    let mut to_restore = Vec::with_capacity(paths.len());
    for p in paths {
        let want = Path::new(p);
        // The most recently trashed item whose original full path matches.
        let best = items
            .iter()
            .filter(|it| it.original_parent.join(&it.name).as_path() == want)
            .max_by_key(|it| it.time_deleted)
            .cloned();
        match best {
            Some(it) => to_restore.push(it),
            None => return Err(FsError::Invalid(format!("Not found in the Recycle Bin: {p}"))),
        }
    }
    restore_all(to_restore).map_err(|e| FsError::Trash(format!("Cannot restore: {e}")))
}

#[cfg(target_os = "macos")]
fn untrash_paths(_paths: &[String]) -> Result<()> {
    Err(FsError::Unsupported(
        "Restoring from the Trash isn't supported on macOS".into(),
    ))
}

#[cfg(not(target_os = "macos"))]
fn trash_list_blocking() -> Result<Vec<TrashEntry>> {
    use trash::os_limited::list;
    let mut items = list().map_err(|e| FsError::Trash(format!("Cannot read the Recycle Bin: {e}")))?;
    // Newest first.
    items.sort_by(|a, b| b.time_deleted.cmp(&a.time_deleted));
    Ok(items
        .into_iter()
        .map(|it| {
            let original = it.original_parent.join(&it.name);
            TrashEntry {
                id:            it.id.to_string_lossy().to_string(),
                name:          it.name.to_string_lossy().to_string(),
                original_path: original.to_string_lossy().to_string(),
                deleted_at:    Some(it.time_deleted),
            }
        })
        .collect())
}

#[cfg(target_os = "macos")]
fn macos_trash_dir() -> Result<std::path::PathBuf> {
    dirs::home_dir()
        .map(|h| h.join(".Trash"))
        .ok_or_else(|| FsError::Invalid("No home directory".into()))
}

#[cfg(target_os = "macos")]
fn trash_list_blocking() -> Result<Vec<TrashEntry>> {
    let dir = macos_trash_dir()?;
    let mut out: Vec<TrashEntry> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == ".DS_Store" { continue; }
            // The mtime in ~/.Trash is the moment the item was trashed.
            let deleted_at = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            out.push(TrashEntry {
                id:            e.path().to_string_lossy().to_string(), // path doubles as the id
                name,
                original_path: String::new(), // not recorded / readable on macOS
                deleted_at,
            });
        }
    }
    out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    Ok(out)
}

/// Recover the given ~/.Trash items to the Desktop (macOS can't Put Back to the
/// original location, which isn't stored). Collision-resolved like a paste.
#[cfg(target_os = "macos")]
fn macos_trash_restore(ids: &[String]) -> Result<()> {
    use std::path::Path;
    let dest = dirs::desktop_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| FsError::Invalid("No restore destination".into()))?;
    for id in ids {
        let src = Path::new(id);
        if !src.exists() { continue; }
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| FsError::Invalid(format!("Invalid item: {id}")))?;
        let target = crate::copy::unique_dest(&dest, &name);
        std::fs::rename(src, &target)
            .map_err(|e| FsError::io(format!("Cannot restore {name}"), e))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_trash_purge(ids: &[String]) -> Result<()> {
    use std::path::Path;
    for id in ids {
        let p = Path::new(id);
        if !p.exists() { continue; }
        let r = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
        r.map_err(|e| FsError::io("Cannot delete", e))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_trash_empty() -> Result<()> {
    // Prefer Finder (handles locked items / permissions); fall back to a direct
    // sweep of ~/.Trash when AppleScript isn't available.
    let scripted = std::process::Command::new("osascript")
        .args(["-e", "tell application \"Finder\" to empty trash"])
        .output();
    if let Ok(o) = scripted {
        if o.status.success() { return Ok(()); }
    }
    let dir = macos_trash_dir()?;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
    }
    Ok(())
}

/// List the items currently in the Recycle Bin / trash (newest first).
pub fn trash_list() -> Result<Vec<TrashEntry>> {
    trash_list_blocking()
}

/// Resolve the requested ids against the current trash listing. Errors when an
/// id no longer matches anything (the item was already restored / purged).
#[cfg(not(target_os = "macos"))]
fn collect_trash_items(ids: &[String]) -> Result<Vec<trash::TrashItem>> {
    use trash::os_limited::list;
    let items = list().map_err(|e| FsError::Trash(format!("Cannot read the Recycle Bin: {e}")))?;
    let want: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let picked: Vec<trash::TrashItem> = items
        .into_iter()
        .filter(|it| want.contains(it.id.to_string_lossy().as_ref()))
        .collect();
    if picked.is_empty() && !ids.is_empty() {
        return Err(FsError::Invalid("Selected items are no longer in the Recycle Bin".into()));
    }
    Ok(picked)
}

/// Restore trashed items (by id). Windows + Linux put them back to their
/// original location; macOS recovers them to the Desktop (no Put-Back API).
pub fn trash_restore(ids: &[String]) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        return macos_trash_restore(ids);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let items = collect_trash_items(ids)?;
        trash::os_limited::restore_all(items)
            .map_err(|e| FsError::Trash(format!("Cannot restore: {e}")))
    }
}

/// Permanently delete trashed items (by id) — they leave the Recycle Bin for
/// good. Windows + Linux + macOS.
pub fn trash_purge(ids: &[String]) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        return macos_trash_purge(ids);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let items = collect_trash_items(ids)?;
        trash::os_limited::purge_all(items)
            .map_err(|e| FsError::Trash(format!("Cannot delete: {e}")))
    }
}

/// Empty the Recycle Bin entirely (permanent). Windows + Linux + macOS.
pub fn trash_empty() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        return macos_trash_empty();
    }
    #[cfg(not(target_os = "macos"))]
    {
        use trash::os_limited::{list, purge_all};
        let items = list().map_err(|e| FsError::Trash(format!("Cannot read the Recycle Bin: {e}")))?;
        if items.is_empty() { return Ok(()); }
        purge_all(items).map_err(|e| FsError::Trash(format!("Cannot empty the Recycle Bin: {e}")))
    }
}
