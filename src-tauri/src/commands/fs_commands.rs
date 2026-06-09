use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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
/// chosen via the in-app FileExplorerModal (e.g. theme imports).
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

/// Build a file-name matcher from `query`. A query containing `*` / `?` is
/// treated as a (case-insensitive, anchored) glob; otherwise it's a
/// case-insensitive substring match. An empty query matches everything.
fn build_name_matcher(query: &str) -> Box<dyn Fn(&str) -> bool + Send> {
    let t = query.trim();
    if t.is_empty() {
        return Box::new(|_| true);
    }
    if t.contains('*') || t.contains('?') {
        let mut re = String::from("(?i)^");
        for ch in t.chars() {
            match ch {
                '*' => re.push_str(".*"),
                '?' => re.push('.'),
                '.' | '+' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\' => {
                    re.push('\\');
                    re.push(ch);
                }
                _ => re.push(ch),
            }
        }
        re.push('$');
        if let Ok(r) = regex::Regex::new(&re) {
            return Box::new(move |name: &str| r.is_match(name));
        }
    }
    let lc = t.to_lowercase();
    Box::new(move |name: &str| name.to_lowercase().contains(&lc))
}

/// Recursively search `root` for entries whose **file name** matches `query`
/// (glob when it contains `*`/`?`, else case-insensitive substring). Returns up
/// to `limit` matches (default 5000) so a huge tree can't run away. Hidden
/// entries (dot-prefixed) are skipped unless `show_hidden`. Each result carries
/// its full path so the explorer can open / reveal / act on it directly.
#[tauri::command]
pub async fn fs_search(
    root: String,
    query: String,
    show_hidden: Option<bool>,
    limit: Option<usize>,
) -> Result<Vec<FsEntry>, AppError> {
    let show_hidden = show_hidden.unwrap_or(false);
    tokio::task::spawn_blocking(move || {
        let cap = limit.unwrap_or(5000);
        let matches = build_name_matcher(&query);
        let mut out: Vec<FsEntry> = Vec::new();
        // Depth-first walk with an explicit stack (no recursion → no stack
        // blow-up on deep trees, and a cheap early-out at the cap).
        let mut stack: Vec<PathBuf> = vec![PathBuf::from(&root)];
        while let Some(dir) = stack.pop() {
            if out.len() >= cap {
                break;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for item in rd {
                let Ok(item) = item else { continue };
                let name = item.file_name().to_string_lossy().to_string();
                if !show_hidden && name.starts_with('.') {
                    continue;
                }
                let full = item.path();
                let meta = item.metadata().ok();
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                if is_dir {
                    stack.push(full.clone());
                }
                if matches(&name) {
                    let size = meta.as_ref().and_then(|m| if m.is_dir() { None } else { Some(m.len()) });
                    let modified = meta
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64);
                    out.push(FsEntry {
                        name,
                        path: full.to_string_lossy().to_string(),
                        is_dir,
                        size,
                        modified,
                    });
                    if out.len() >= cap {
                        break;
                    }
                }
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_search task panicked: {e}")))?
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

// ---------------------------------------------------------------------------
// Copy / move (clipboard paste) — used by the built-in file explorer.
// ---------------------------------------------------------------------------

/// Pick a non-colliding destination path inside `dir` for an entry named
/// `name`, appending " (2)", " (3)", … before the extension on collision —
/// mirroring Windows Explorer's paste behaviour.
fn unique_dest(dir: &Path, name: &str) -> PathBuf {
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

fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// Copy each of `sources` into `dest_dir`, resolving name collisions. Returns
/// the list of created destination paths. Recursive for directories.
#[tauri::command]
pub async fn fs_copy(sources: Vec<String>, dest_dir: String) -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(move || {
        let dir = Path::new(&dest_dir);
        let mut created = Vec::with_capacity(sources.len());
        for s in &sources {
            let src = Path::new(s);
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .ok_or_else(|| AppError::Other(format!("Invalid source path: {s}")))?;
            let dst = unique_dest(dir, &name);
            copy_recursive(src, &dst)
                .map_err(|e| AppError::Other(format!("Cannot copy {name}: {e}")))?;
            created.push(dst.to_string_lossy().to_string());
        }
        Ok(created)
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_copy task panicked: {e}")))?
}

/// Move each of `sources` into `dest_dir` (cut + paste). Falls back to
/// copy-then-delete across volumes where `rename` can't work. Returns the
/// list of new paths. Moving into the same directory is a no-op.
#[tauri::command]
pub async fn fs_move(sources: Vec<String>, dest_dir: String) -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(move || {
        let dir = Path::new(&dest_dir);
        let mut moved = Vec::with_capacity(sources.len());
        for s in &sources {
            let src = Path::new(s);
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .ok_or_else(|| AppError::Other(format!("Invalid source path: {s}")))?;
            // No-op when the destination is the source's own parent.
            if src.parent() == Some(dir) {
                moved.push(s.clone());
                continue;
            }
            // Refuse to move a directory into itself or a descendant.
            if dir.starts_with(src) {
                return Err(AppError::Other("Cannot move a folder into itself".into()));
            }
            let dst = unique_dest(dir, &name);
            if std::fs::rename(src, &dst).is_err() {
                copy_recursive(src, &dst)
                    .map_err(|e| AppError::Other(format!("Cannot move {name}: {e}")))?;
                let removed = if src.is_dir() {
                    std::fs::remove_dir_all(src)
                } else {
                    std::fs::remove_file(src)
                };
                removed.map_err(|e| AppError::Other(format!("Cannot remove source {name}: {e}")))?;
            }
            moved.push(dst.to_string_lossy().to_string());
        }
        Ok(moved)
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_move task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Delete — trash (default) vs permanent (Shift+Delete)
// ---------------------------------------------------------------------------

/// Move paths to the OS trash / Recycle Bin (recoverable).
#[tauri::command]
pub async fn fs_trash(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        trash::delete_all(&paths).map_err(|e| AppError::Other(format!("Cannot move to trash: {e}")))
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_trash task panicked: {e}")))?
}

/// Permanently delete paths from disk (files or directories). Not recoverable.
#[tauri::command]
pub async fn fs_delete_many(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        for p in &paths {
            let path = Path::new(p);
            let r = if path.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            };
            r.map_err(|e| AppError::Other(format!("Cannot delete {p}: {e}")))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_delete_many task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Compress / extract — ZIP archives (backed by the `zip` crate, already a dep)
// ---------------------------------------------------------------------------

/// Recursively add `path` to `zip`, naming entries relative to `base` (the
/// parent of the top-level item) with forward slashes (the ZIP convention).
fn zip_add_entry(
    zip: &mut zip::ZipWriter<std::fs::File>,
    base: &Path,
    path: &Path,
    opts: zip::write::SimpleFileOptions,
) -> std::io::Result<()> {
    let rel = path.strip_prefix(base).unwrap_or(path);
    let name = rel.to_string_lossy().replace('\\', "/");
    if path.is_dir() {
        // Store the directory itself so empty folders survive the round-trip.
        if !name.is_empty() {
            zip.add_directory(format!("{name}/"), opts)?;
        }
        for entry in std::fs::read_dir(path)? {
            zip_add_entry(zip, base, &entry?.path(), opts)?;
        }
    } else {
        zip.start_file(name, opts)?;
        let mut f = std::fs::File::open(path)?;
        std::io::copy(&mut f, zip)?;
    }
    Ok(())
}

/// Compress `sources` into a new ZIP archive named `archive_name` inside
/// `dest_dir` (collision-resolved). Returns the created archive path. Each
/// source keeps its own name as the top-level entry; directories recurse.
#[tauri::command]
pub async fn fs_zip(
    sources: Vec<String>,
    dest_dir: String,
    archive_name: String,
) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        if sources.is_empty() {
            return Err(AppError::Other("Nothing to compress".into()));
        }
        let dir = Path::new(&dest_dir);
        let out = unique_dest(dir, &archive_name);
        let file = std::fs::File::create(&out)
            .map_err(|e| AppError::Other(format!("Cannot create archive: {e}")))?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for s in &sources {
            let src = Path::new(s);
            let base = src.parent().unwrap_or(dir);
            zip_add_entry(&mut zip, base, src, opts)
                .map_err(|e| AppError::Other(format!("Cannot add {s} to archive: {e}")))?;
        }
        zip.finish()
            .map_err(|e| AppError::Other(format!("Cannot finalize archive: {e}")))?;
        Ok(out.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_zip task panicked: {e}")))?
}

/// Extract a ZIP `archive` into `dest_dir`, or — when `dest_dir` is omitted —
/// into a new sibling folder named after the archive (collision-resolved).
/// Entry names are sanitised via `enclosed_name` to defeat path-traversal
/// ("zip slip"). Returns the destination folder path.
#[tauri::command]
pub async fn fs_unzip(archive: String, dest_dir: Option<String>) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let arch = Path::new(&archive);
        let file = std::fs::File::open(arch)
            .map_err(|e| AppError::Other(format!("Cannot open archive: {e}")))?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|e| AppError::Other(format!("Not a valid ZIP archive: {e}")))?;

        let out_dir = match dest_dir {
            Some(d) => PathBuf::from(d),
            None => {
                let parent = arch.parent().unwrap_or_else(|| Path::new("."));
                let stem = arch
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "extracted".to_string());
                unique_dest(parent, &stem)
            }
        };
        std::fs::create_dir_all(&out_dir)
            .map_err(|e| AppError::Other(format!("Cannot create output folder: {e}")))?;

        for i in 0..zip.len() {
            let mut entry = zip
                .by_index(i)
                .map_err(|e| AppError::Other(format!("Cannot read archive entry: {e}")))?;
            // Skip entries with unsafe names (absolute paths / `..` traversal).
            let Some(rel) = entry.enclosed_name() else { continue };
            let outpath = out_dir.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| AppError::Other(format!("Cannot create dir: {e}")))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| AppError::Other(format!("Cannot create dir: {e}")))?;
                }
                let mut out = std::fs::File::create(&outpath)
                    .map_err(|e| AppError::Other(format!("Cannot write file: {e}")))?;
                std::io::copy(&mut entry, &mut out)
                    .map_err(|e| AppError::Other(format!("Cannot extract file: {e}")))?;
            }
        }
        Ok(out_dir.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_unzip task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Open / reveal via the OS (default app, file manager)
// ---------------------------------------------------------------------------

/// Open a path with the OS default application (file) or file manager (dir).
#[tauri::command]
pub fn fs_open_default(app: tauri::AppHandle, path: String) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| AppError::Other(format!("Cannot open: {e}")))
}

/// Reveal a path in the OS file manager (Explorer / Finder), selecting it.
#[tauri::command]
pub fn fs_reveal_in_dir(app: tauri::AppHandle, path: String) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| AppError::Other(format!("Cannot reveal: {e}")))
}

/// Open the OS-native file/folder **Properties** dialog for `path`.
///
/// Cross-platform, each backed by the platform's own shell facility:
/// - **Windows** → `SHObjectProperties` property sheet, on a detached thread
///   (the sheet manages its own lifetime, so the call returns immediately).
/// - **macOS** → Finder *Get Info* window via AppleScript (`osascript`).
/// - **Linux/BSD** → freedesktop `org.freedesktop.FileManager1.ShowItemProperties`
///   over D-Bus (`gdbus`), honoured by Nautilus, Dolphin, Nemo, …
///
/// Never blocks the UI thread. The in-app Info panel stays as the fallback
/// when the platform facility is unavailable.
#[tauri::command]
pub fn fs_show_properties(path: String) -> Result<(), AppError> {
    if !Path::new(&path).exists() {
        return Err(AppError::Other(format!("Path does not exist: {path}")));
    }

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
        };
        use windows_sys::Win32::UI::Shell::SHObjectProperties;

        // `szObject` is a file-system path (vs. a pidl / printer / etc.).
        const SHOP_FILEPATH: u32 = 0x0000_0002;

        let wide: Vec<u16> = OsStr::new(&path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        std::thread::spawn(move || unsafe {
            // STA: the property sheet hosts shell extension pages that expect
            // an apartment-threaded COM context.
            let hr = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            SHObjectProperties(
                0,
                SHOP_FILEPATH,
                wide.as_ptr(),
                std::ptr::null(),
            );
            // Balance CoInitializeEx only when it actually initialised COM
            // (S_OK / S_FALSE ≥ 0); RPC_E_CHANGED_MODE is negative → skip.
            if hr >= 0 {
                CoUninitialize();
            }
        });
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        // Finder "Get Info" window. Escape backslashes then quotes so the path
        // is a safe AppleScript string literal.
        let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "tell application \"Finder\"\n\
             activate\n\
             open information window of (POSIX file \"{escaped}\" as alias)\n\
             end tell"
        );
        spawn_detached_reap(
            std::process::Command::new("osascript").arg("-e").arg(script),
        )
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // freedesktop FileManager1: ShowItemProperties(uris: as, startup_id: s).
        let uri = path_to_file_uri(&path);
        let uris_arg = format!("['{}']", uri.replace('\'', "%27"));
        spawn_detached_reap(std::process::Command::new("gdbus").args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.FileManager1",
            "--object-path",
            "/org/freedesktop/FileManager1",
            "--method",
            "org.freedesktop.FileManager1.ShowItemProperties",
            &uris_arg,
            "",
        ]))
    }

    #[cfg(not(any(windows, target_os = "macos", all(unix, not(target_os = "macos")))))]
    {
        let _ = path;
        Err(AppError::Other(
            "Native properties dialog is not supported on this platform".into(),
        ))
    }
}

/// Spawn a short-lived helper that triggers a native dialog in another process,
/// then reap it on a background thread so it never zombifies and never blocks
/// the UI thread. Only a spawn failure (missing binary) is surfaced to the
/// caller; runtime failures are the helper's concern.
#[cfg(unix)]
fn spawn_detached_reap(cmd: &mut std::process::Command) -> Result<(), AppError> {
    let child = cmd
        .spawn()
        .map_err(|e| AppError::Other(format!("Cannot open properties: {e}")))?;
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(())
}

/// Percent-encode an absolute path into a `file://` URI (keeps `/`, encodes
/// spaces and other reserved bytes) for the FileManager1 D-Bus call.
#[cfg(all(unix, not(target_os = "macos")))]
fn path_to_file_uri(path: &str) -> String {
    const UNRESERVED: &[u8] = b"-_.~/";
    let mut out = String::from("file://");
    for &b in path.as_bytes() {
        if b.is_ascii_alphanumeric() || UNRESERVED.contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Set desktop wallpaper — image-file context-menu action
// ---------------------------------------------------------------------------

/// Set `path` (an image file) as the desktop wallpaper.
///
/// - **Windows** → `SystemParametersInfoW(SPI_SETDESKWALLPAPER)` (persists the
///   choice to the user profile and broadcasts the change to the shell).
/// - **macOS** → System Events AppleScript, applied to every desktop.
/// - **Linux/BSD** → GNOME `gsettings` (`picture-uri` + `picture-uri-dark`);
///   best-effort, other desktop environments aren't covered.
#[tauri::command]
pub fn fs_set_wallpaper(path: String) -> Result<(), AppError> {
    if !Path::new(&path).exists() {
        return Err(AppError::Other(format!("Path does not exist: {path}")));
    }

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
        };
        let wide: Vec<u16> = OsStr::new(&path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_SETDESKWALLPAPER,
                0,
                wide.as_ptr() as *mut core::ffi::c_void,
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            )
        };
        if ok == 0 {
            return Err(AppError::Other("Failed to set desktop wallpaper".into()));
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "tell application \"System Events\" to set picture of every desktop to \"{escaped}\""
        );
        spawn_detached_reap(std::process::Command::new("osascript").arg("-e").arg(script))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let uri = path_to_file_uri(&path);
        // GNOME splits the light/dark wallpaper keys; set both so the choice
        // sticks regardless of the active color scheme.
        for key in ["picture-uri", "picture-uri-dark"] {
            let _ = std::process::Command::new("gsettings")
                .args(["set", "org.gnome.desktop.background", key, &uri])
                .status();
        }
        Ok(())
    }

    #[cfg(not(any(windows, target_os = "macos", all(unix, not(target_os = "macos")))))]
    {
        let _ = path;
        Err(AppError::Other(
            "Setting the desktop wallpaper is not supported on this platform".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Native system icons — the real shell/desktop icon for a file extension,
// returned as a PNG data-URI the explorer renders in an <img>. Backed by the
// `systemicons` crate. Folders are NOT covered (the crate resolves icons by a
// `FILE_ATTRIBUTE_NORMAL` query), so the explorer keeps its themed folder icon.
//
// Threading: GTK (Linux) / AppKit (macOS) icon lookups must run on the main
// thread → we marshal there via `run_on_main_thread`. The Windows lookup may
// block (it retries with short sleeps), so it runs on a blocking worker
// instead, keeping the main thread responsive.
// ---------------------------------------------------------------------------

/// Native system icon for `query`, as a `data:image/png;base64,…` URI.
///
/// `query` is normally a file extension (e.g. `".rs"`) so results cache by
/// type; an absolute path to an `.exe` yields its embedded resource icon.
/// `size` is the desired pixel size (e.g. 32 for a crisp 16px render).
#[tauri::command]
pub async fn fs_icon(app: tauri::AppHandle, query: String, size: i32) -> Result<String, AppError> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    let png = icon_png_bytes(app, query, size).await?;
    Ok(format!("data:image/png;base64,{}", BASE64.encode(&png)))
}

#[cfg(windows)]
async fn icon_png_bytes(
    _app: tauri::AppHandle,
    query: String,
    size: i32,
) -> Result<Vec<u8>, AppError> {
    tokio::task::spawn_blocking(move || {
        systemicons::get_icon(&query, size)
            .map_err(|e| AppError::Other(format!("Cannot load icon: {}", e.message)))
    })
    .await
    .map_err(|e| AppError::Other(format!("icon task failed: {e}")))?
}

#[cfg(not(windows))]
async fn icon_png_bytes(
    app: tauri::AppHandle,
    query: String,
    size: i32,
) -> Result<Vec<u8>, AppError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let res = systemicons::get_icon(&query, size).map_err(|e| e.message);
        let _ = tx.send(res);
    })
    .map_err(|e| AppError::Other(format!("icon dispatch failed: {e}")))?;
    rx.await
        .map_err(|e| AppError::Other(format!("icon channel closed: {e}")))?
        .map_err(|m| AppError::Other(format!("Cannot load icon: {m}")))
}

// ---------------------------------------------------------------------------
// Filesystem watcher — live updates for the explorer's current directory.
// One watcher PER WINDOW, keyed by window label: every explorer window (the
// canonical `explorer`, any `explorer-N`, and the in-app modal hosted by
// `main`) watches its own current directory independently. Starting a watch
// replaces only the calling window's previous watcher; stopping removes only
// its entry. The change signal is emitted to the originating window ALONE
// (`emit_to(label)`) — never broadcast — so one window's change can't make
// every other explorer window refetch + recompute git status. Coalescing
// happens on the frontend; the payload is empty (just "something changed").
// ---------------------------------------------------------------------------

fn watchers() -> &'static Mutex<HashMap<String, notify::RecommendedWatcher>> {
    static SLOT: OnceLock<Mutex<HashMap<String, notify::RecommendedWatcher>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(HashMap::new()))
}

#[tauri::command]
pub fn fs_watch_start(window: tauri::WebviewWindow, path: String) -> Result<(), AppError> {
    use notify::{RecursiveMode, Watcher};
    use tauri::{Emitter, Manager};

    let app = window.app_handle().clone();
    let label = window.label().to_string();
    let emit_label = label.clone();
    // Source-side throttle: an active directory (a build writing files, a repo's
    // index churning) makes the OS watcher fire hundreds of events per second.
    // Emitting one IPC message per raw event floods the owning window's event
    // loop — the frontend debounces the *refresh*, but the `listen` callback
    // itself still fires per message, saturating the renderer (the window stops
    // responding, drag included) and, with several explorer windows open, every
    // renderer at once. Collapse bursts to at most one signal per interval; the
    // frontend's own 200 ms debounce coalesces the rest.
    let mut last_emit: Option<std::time::Instant> = None;
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let now = std::time::Instant::now();
            let due = last_emit.map_or(true, |t| now.duration_since(t) >= std::time::Duration::from_millis(120));
            if !due { return; }
            last_emit = Some(now);
            // Target the owning window only — broadcasting would refresh every
            // open explorer window (each doing a full git recompute) on one
            // window's change.
            let _ = app.emit_to(emit_label.as_str(), "arbor://fs-changed", ());
        }
    })
    .map_err(|e| AppError::Other(format!("Cannot create watcher: {e}")))?;

    watcher
        .watch(Path::new(&path), RecursiveMode::NonRecursive)
        .map_err(|e| AppError::Other(format!("Cannot watch path: {e}")))?;

    // Drop watchers whose window has since closed (no reliable stop on window
    // teardown), then store this window's — replacing its own previous watch
    // without touching any other window's.
    let live: std::collections::HashSet<String> = window.app_handle().webview_windows().into_keys().collect();
    let mut map = watchers()
        .lock()
        .map_err(|_| AppError::Other("watcher map poisoned".into()))?;
    map.retain(|k, _| live.contains(k));
    map.insert(label, watcher);
    Ok(())
}

#[tauri::command]
pub fn fs_watch_stop(window: tauri::WebviewWindow) {
    if let Ok(mut map) = watchers().lock() {
        map.remove(window.label());
    }
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
