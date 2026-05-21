use serde::Serialize;
use std::path::Path;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Mutating filesystem operations
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn fs_create_dir(path: String) -> Result<(), AppError> {
    std::fs::create_dir_all(&path)
        .map_err(|e| AppError::Other(format!("Cannot create directory: {e}")))
}

#[tauri::command]
pub fn fs_create_file(path: String) -> Result<(), AppError> {
    // Create parent dirs if needed, then create an empty file.
    if let Some(parent) = Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::File::create(&path)
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("Cannot create file: {e}")))
}

#[tauri::command]
pub fn fs_rename(old_path: String, new_path: String) -> Result<(), AppError> {
    std::fs::rename(&old_path, &new_path)
        .map_err(|e| AppError::Other(format!("Cannot rename: {e}")))
}

/// Write a text file, creating it (or overwriting it) at the given path.
/// Parent directories are created automatically if they don't exist.
#[tauri::command]
pub fn fs_write_text_file(path: String, content: String) -> Result<(), AppError> {
    if let Some(parent) = Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, content.as_bytes())
        .map_err(|e| AppError::Other(format!("Cannot write file: {e}")))
}

/// Read a text file from disk and return its contents as a UTF-8 string.
/// Errors out for non-UTF-8 files; suitable for JSON / TOML / config files
/// chosen via the in-app FilePickerModal (e.g. theme imports).
#[tauri::command]
pub fn fs_read_text_file(path: String) -> Result<String, AppError> {
    std::fs::read_to_string(&path)
        .map_err(|e| AppError::Other(format!("Cannot read file: {e}")))
}

/// Delete a file or directory (recursively for dirs).
#[tauri::command]
pub fn fs_delete(path: String) -> Result<(), AppError> {
    let p = Path::new(&path);
    let result = if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    };
    result.map_err(|e| AppError::Other(format!("Cannot delete: {e}")))
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct FsEntry {
    pub name:     String,
    pub path:     String,
    pub is_dir:   bool,
    /// File size in bytes. `None` for directories or on error.
    pub size:     Option<u64>,
    /// Last-modified time as Unix timestamp in milliseconds. `None` on error.
    pub modified: Option<i64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct FsRoot {
    pub name: String,
    pub path: String,
    /// "home" | "desktop" | "documents" | "downloads" | "drive"
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn read_dir_blocking(path: String, show_hidden: bool) -> Result<Vec<FsEntry>, AppError> {
    let base = Path::new(&path);

    let read = std::fs::read_dir(base)
        .map_err(|e| AppError::Other(format!("Cannot read directory: {e}")))?;

    let mut entries: Vec<FsEntry> = Vec::new();

    for item in read {
        let Ok(item) = item else { continue };

        let name = item.file_name().to_string_lossy().to_string();

        // Skip hidden entries (dot-prefixed) when not explicitly requested.
        if !show_hidden && name.starts_with('.') { continue; }

        let full_path = item.path();
        let path_str  = full_path.to_string_lossy().to_string();

        let (is_dir, size, modified) = match item.metadata() {
            Ok(meta) => {
                let is_dir = meta.is_dir();
                let size   = if is_dir { None } else { Some(meta.len()) };
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64);
                (is_dir, size, modified)
            }
            Err(_) => (full_path.is_dir(), None, None),
        };

        entries.push(FsEntry { name, path: path_str, is_dir, size, modified });
    }

    Ok(entries)
}

/// List the contents of a directory with full metadata in a single call.
/// Hidden entries (starting with `.`) are excluded unless `show_hidden` is set.
/// Runs on the blocking pool so a slow network drive or large dir doesn't
/// stall the IPC runtime.
#[tauri::command]
pub async fn fs_read_dir(path: String, show_hidden: Option<bool>) -> Result<Vec<FsEntry>, AppError> {
    let show_hidden = show_hidden.unwrap_or(false);
    tokio::task::spawn_blocking(move || read_dir_blocking(path, show_hidden))
        .await
        .map_err(|e| AppError::Other(format!("fs_read_dir task panicked: {e}")))?
}

#[cfg(target_os = "windows")]
fn enumerate_drives() -> Vec<FsRoot> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

    let mut drives = Vec::new();
    // GetLogicalDrives returns a bitmask: bit 0 = A:, bit 1 = B:, …, bit 25 = Z:
    // It's a single fast Win32 call that reads from the system without
    // probing each drive — replacing the old A..Z + Path::exists() loop
    // which blocked for several seconds per unavailable removable/CD drive.
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 { return drives; }

    for i in 0..26 {
        if mask & (1u32 << i) != 0 {
            let letter = (b'A' + i as u8) as char;
            drives.push(FsRoot {
                name: format!("{letter}:"),
                path: format!("{letter}:\\"),
                kind: "drive".to_string(),
            });
        }
    }
    drives
}

fn list_fs_roots_blocking() -> Vec<FsRoot> {
    let mut roots: Vec<FsRoot> = Vec::new();

    // ── Common user directories ───────────────────────────────────────────
    let common = [
        (dirs::home_dir(),      "Home",      "home"),
        (dirs::desktop_dir(),   "Desktop",   "desktop"),
        (dirs::document_dir(),  "Documents", "documents"),
        (dirs::download_dir(),  "Downloads", "downloads"),
    ];

    for (opt, name, kind) in common {
        if let Some(p) = opt {
            if p.exists() {
                roots.push(FsRoot {
                    name: name.to_string(),
                    path: p.to_string_lossy().to_string(),
                    kind: kind.to_string(),
                });
            }
        }
    }

    // ── Platform-specific drives / root ───────────────────────────────────
    #[cfg(target_os = "windows")]
    {
        roots.extend(enumerate_drives());
    }

    #[cfg(not(target_os = "windows"))]
    {
        roots.push(FsRoot {
            name: "File System".to_string(),
            path: "/".to_string(),
            kind: "drive".to_string(),
        });
    }

    roots
}

/// Return filesystem quick-access roots:
/// - On Windows: common user dirs (Home, Desktop, Documents, Downloads)
///   followed by available drive letters (C:\, D:\, …).
/// - On other platforms: Home, Desktop, Documents, Downloads, and `/`.
///
/// Runs on the blocking pool because `dirs::*_dir()` + `Path::exists()` can
/// touch the filesystem (and on Windows the previous A..Z probe blocked the
/// IPC thread for seconds at a time on machines with offline removable
/// drives). The Win32 fast path uses `GetLogicalDrives` and returns instantly.
#[tauri::command]
pub async fn list_fs_roots() -> Vec<FsRoot> {
    tokio::task::spawn_blocking(list_fs_roots_blocking)
        .await
        .unwrap_or_default()
}
