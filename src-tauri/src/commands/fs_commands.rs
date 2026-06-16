use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tauri::Emitter;

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

/// One old→new rename pair for a batch rename.
#[derive(Debug, serde::Deserialize)]
pub struct RenamePair {
    pub from: String,
    pub to:   String,
}

/// Batch-rename in two phases so order-independent shuffles (e.g. `a→b, b→c`,
/// or swapping two names) don't clobber each other: every source is first moved
/// to a unique temp name, then to its final name. Validates up front that the
/// final names are unique and don't collide with files left untouched. All
/// targets share the parent of their source.
#[tauri::command]
pub fn fs_rename_many(pairs: Vec<RenamePair>) -> Result<Vec<String>, AppError> {
    use std::collections::HashSet;
    // Reject duplicate destinations early — two files can't take the same name.
    let mut seen = HashSet::new();
    for p in &pairs {
        let to = Path::new(&p.to);
        let parent = to.parent().unwrap_or_else(|| Path::new(""));
        let key = parent.join(to.file_name().unwrap_or_default());
        if !seen.insert(key) {
            return Err(AppError::Other(format!(
                "Two items would be renamed to the same name: {}",
                to.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            )));
        }
    }
    // Reject a destination that already exists and isn't itself being renamed
    // away in this batch (so shifting a contiguous block is fine, overwriting an
    // unrelated file is not).
    let froms: HashSet<PathBuf> = pairs.iter().map(|p| PathBuf::from(&p.from)).collect();
    for p in &pairs {
        let to = Path::new(&p.to);
        if to.exists() && !froms.contains(to) {
            return Err(AppError::Other(format!(
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
            .map_err(|e| AppError::Other(format!("Cannot rename {}: {e}", p.from)))?;
        temps.push((tmp, p.to.clone()));
    }
    // Phase 2: temp → final.
    let mut out = Vec::with_capacity(temps.len());
    for (tmp, to) in &temps {
        std::fs::rename(tmp, to)
            .map_err(|e| AppError::Other(format!("Cannot rename to {to}: {e}")))?;
        out.push(to.clone());
    }
    Ok(out)
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
    /// Creation time as Unix timestamp in milliseconds. `None` on error or on
    /// platforms/filesystems that don't record a birth time (e.g. many Linux FS).
    pub created:  Option<i64>,
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

/// Convert a filesystem timestamp into a Unix-epoch value in milliseconds,
/// swallowing the `io::Result` and any pre-epoch / unsupported cases to `None`.
fn to_unix_ms(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    t.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
}

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

        let (is_dir, size, modified, created) = match item.metadata() {
            Ok(meta) => {
                let is_dir = meta.is_dir();
                let size   = if is_dir { None } else { Some(meta.len()) };
                (is_dir, size, to_unix_ms(meta.modified()), to_unix_ms(meta.created()))
            }
            Err(_) => (full_path.is_dir(), None, None, None),
        };

        entries.push(FsEntry { name, path: path_str, is_dir, size, modified, created });
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
                    let modified = meta.as_ref().and_then(|m| to_unix_ms(m.modified()));
                    let created  = meta.as_ref().and_then(|m| to_unix_ms(m.created()));
                    out.push(FsEntry {
                        name,
                        path: full.to_string_lossy().to_string(),
                        is_dir,
                        size,
                        modified,
                        created,
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

// ---------------------------------------------------------------------------
// Progress + cancellation for long-running file operations (copy / move /
// duplicate). A per-`op_id` cancel flag lets the UI abort a running op; a
// throttled `arbor://fs-op-progress` event drives the explorer's progress bar.
// ---------------------------------------------------------------------------

/// Process-wide registry of cancel flags keyed by op_id.
fn cancel_registry() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_op(op_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut m) = cancel_registry().lock() {
        m.insert(op_id.to_string(), flag.clone());
    }
    flag
}
fn unregister_op(op_id: &str) {
    if let Ok(mut m) = cancel_registry().lock() {
        m.remove(op_id);
    }
}

/// Request cancellation of a running file operation. No-op for unknown ids
/// (the op may have already finished). Cooperative: the op stops at the next
/// file boundary.
#[tauri::command]
pub fn fs_cancel_op(op_id: String) {
    if let Ok(m) = cancel_registry().lock() {
        if let Some(flag) = m.get(&op_id) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

#[derive(Clone, Serialize)]
struct FsOpProgress {
    op_id:       String,
    kind:        String,
    done_files:  u64,
    total_files: u64,
    done_bytes:  u64,
    total_bytes: u64,
    current:     String,
}

/// Drives progress emission + cancellation for one file operation. When `app`
/// is `None` (no op_id supplied) every method is a cheap no-op, so the copy
/// path is identical whether or not the caller wants progress.
struct OpProgress {
    app:         Option<tauri::AppHandle>,
    op_id:       String,
    kind:        &'static str,
    total_files: u64,
    total_bytes: u64,
    done_files:  u64,
    done_bytes:  u64,
    last_emit:   Instant,
    cancel:      Arc<AtomicBool>,
}

impl OpProgress {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    /// Emit a progress event, throttled to ~80ms unless `force`d (used for the
    /// initial and final frames so the bar starts and reaches 100%).
    fn emit(&mut self, current: &str, force: bool) {
        if self.app.is_none() {
            return;
        }
        if !force && self.last_emit.elapsed().as_millis() < 80 {
            return;
        }
        self.last_emit = Instant::now();
        let payload = FsOpProgress {
            op_id:       self.op_id.clone(),
            kind:        self.kind.to_string(),
            done_files:  self.done_files,
            total_files: self.total_files,
            done_bytes:  self.done_bytes,
            total_bytes: self.total_bytes,
            current:     current.to_string(),
        };
        if let Some(app) = &self.app {
            let _ = app.emit("arbor://fs-op-progress", payload);
        }
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

/// Sentinel error used to mark a user-cancelled operation, so callers can swap
/// it for a friendly outcome instead of a red error toast.
const CANCELLED_MSG: &str = "Operation cancelled";

/// Progress- and cancel-aware recursive copy. Mirrors `copy_recursive` but
/// reports each copied file and bails out (with `CANCELLED_MSG`) when the op's
/// cancel flag is set.
fn copy_tree(src: &Path, dst: &Path, prog: &mut OpProgress) -> Result<(), AppError> {
    if prog.cancelled() {
        return Err(AppError::Other(CANCELLED_MSG.into()));
    }
    let meta = std::fs::symlink_metadata(src)
        .map_err(|e| AppError::Other(format!("Cannot read {}: {e}", src.display())))?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst)
            .map_err(|e| AppError::Other(format!("Cannot create {}: {e}", dst.display())))?;
        for entry in std::fs::read_dir(src)
            .map_err(|e| AppError::Other(format!("Cannot read {}: {e}", src.display())))?
        {
            let entry = entry.map_err(|e| AppError::Other(format!("{e}")))?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()), prog)?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::copy(src, dst)
            .map_err(|e| AppError::Other(format!("Cannot copy {}: {e}", src.display())))?;
        prog.done_files += 1;
        prog.done_bytes += meta.len();
        let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        prog.emit(&name, false);
    }
    Ok(())
}

/// Build an `OpProgress` for an operation. With an `op_id` it registers a
/// cancel flag, pre-scans `sources` for the totals, and emits live; without
/// one it's a no-op tracker (the fast path for small / scriptless calls).
fn make_progress(
    app: &tauri::AppHandle,
    op_id: &Option<String>,
    kind: &'static str,
    sources: &[String],
) -> OpProgress {
    match op_id {
        Some(id) => {
            let cancel = register_op(id);
            let (mut tf, mut tb) = (0u64, 0u64);
            for s in sources { scan_totals(Path::new(s), &mut tf, &mut tb); }
            let mut p = OpProgress {
                app: Some(app.clone()), op_id: id.clone(), kind,
                total_files: tf, total_bytes: tb, done_files: 0, done_bytes: 0,
                last_emit: Instant::now(), cancel,
            };
            p.emit("", true); // initial frame so the bar appears at 0%
            p
        }
        None => OpProgress {
            app: None, op_id: String::new(), kind,
            total_files: 0, total_bytes: 0, done_files: 0, done_bytes: 0,
            last_emit: Instant::now(), cancel: Arc::new(AtomicBool::new(false)),
        },
    }
}

/// Copy each of `sources` into `dest_dir`. With `overwrite = false` (default)
/// name collisions are resolved Explorer-style (" (2)", " (3)", …); with
/// `overwrite = true` each item keeps its name and merges into any existing
/// folder of the same name, replacing colliding files (recursive). Returns the
/// list of created / merged destination paths.
#[tauri::command]
pub async fn fs_copy(
    app: tauri::AppHandle,
    sources: Vec<String>,
    dest_dir: String,
    overwrite: Option<bool>,
    op_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    let overwrite = overwrite.unwrap_or(false);
    tokio::task::spawn_blocking(move || {
        let mut prog = make_progress(&app, &op_id, "copy", &sources);
        let result: Result<Vec<String>, AppError> = (|| {
            let dir = Path::new(&dest_dir);
            let mut created = Vec::with_capacity(sources.len());
            for s in &sources {
                let src = Path::new(s);
                let name = src
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .ok_or_else(|| AppError::Other(format!("Invalid source path: {s}")))?;
                let dst = if overwrite {
                    dir.join(&name)
                } else {
                    unique_dest(dir, &name)
                };
                copy_tree(src, &dst, &mut prog)?;
                created.push(dst.to_string_lossy().to_string());
            }
            prog.emit("", true); // final 100% frame
            Ok(created)
        })();
        if let Some(id) = &op_id { unregister_op(id); }
        result
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_copy task panicked: {e}")))?
}

/// Move each of `sources` into `dest_dir` (cut + paste). Falls back to
/// copy-then-delete across volumes where `rename` can't work. With
/// `overwrite = true` an item keeps its name and merges into / replaces an
/// existing same-named entry instead of getting a " (2)" suffix. Returns the
/// list of new paths. Moving into the same directory is a no-op.
#[tauri::command]
pub async fn fs_move(
    app: tauri::AppHandle,
    sources: Vec<String>,
    dest_dir: String,
    overwrite: Option<bool>,
    op_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    let overwrite = overwrite.unwrap_or(false);
    tokio::task::spawn_blocking(move || {
        // Same-volume moves are instant renames (no progress); only the
        // cross-volume copy+delete fallback reports progress, so we still build
        // the tracker up front (it pre-scans the totals for that case).
        let mut prog = make_progress(&app, &op_id, "move", &sources);
        let result: Result<Vec<String>, AppError> = (|| {
            let dir = Path::new(&dest_dir);
            let mut moved = Vec::with_capacity(sources.len());
            for s in &sources {
                if prog.cancelled() {
                    return Err(AppError::Other(CANCELLED_MSG.into()));
                }
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
                let dst = if overwrite {
                    dir.join(&name)
                } else {
                    unique_dest(dir, &name)
                };
                if std::fs::rename(src, &dst).is_err() {
                    copy_tree(src, &dst, &mut prog)?;
                    let removed = if src.is_dir() {
                        std::fs::remove_dir_all(src)
                    } else {
                        std::fs::remove_file(src)
                    };
                    removed.map_err(|e| AppError::Other(format!("Cannot remove source {name}: {e}")))?;
                }
                moved.push(dst.to_string_lossy().to_string());
            }
            prog.emit("", true);
            Ok(moved)
        })();
        if let Some(id) = &op_id { unregister_op(id); }
        result
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_move task panicked: {e}")))?
}

/// Duplicate each of `sources` in place (same parent folder), Explorer-style:
/// `report.pdf` → `report (2).pdf`, a second time → `report (3).pdf`. Returns
/// the created paths. Progress/cancel work exactly like `fs_copy`.
#[tauri::command]
pub async fn fs_duplicate(
    app: tauri::AppHandle,
    paths: Vec<String>,
    op_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(move || {
        let mut prog = make_progress(&app, &op_id, "duplicate", &paths);
        let result: Result<Vec<String>, AppError> = (|| {
            let mut created = Vec::with_capacity(paths.len());
            for s in &paths {
                let src = Path::new(s);
                let parent = src.parent()
                    .ok_or_else(|| AppError::Other(format!("Cannot duplicate a root path: {s}")))?;
                let name = src.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .ok_or_else(|| AppError::Other(format!("Invalid source path: {s}")))?;
                let dst = unique_dest(parent, &name);
                copy_tree(src, &dst, &mut prog)?;
                created.push(dst.to_string_lossy().to_string());
            }
            prog.emit("", true);
            Ok(created)
        })();
        if let Some(id) = &op_id { unregister_op(id); }
        result
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_duplicate task panicked: {e}")))?
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

/// Restore previously-trashed entries back to their original locations — the
/// "undo" of `fs_trash`. For each requested original path, the most recently
/// deleted matching item in the Recycle Bin is restored. Backed by the OS trash
/// index (Windows / Linux); not supported on macOS, where the trash exposes no
/// restore API.
#[tauri::command]
pub async fn fs_untrash(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || untrash_paths(paths))
        .await
        .map_err(|e| AppError::Other(format!("fs_untrash task panicked: {e}")))?
}

#[cfg(not(target_os = "macos"))]
fn untrash_paths(paths: Vec<String>) -> Result<(), AppError> {
    use trash::os_limited::{list, restore_all};

    let items =
        list().map_err(|e| AppError::Other(format!("Cannot read the Recycle Bin: {e}")))?;
    let mut to_restore = Vec::with_capacity(paths.len());
    for p in &paths {
        let want = Path::new(p);
        // The most recently trashed item whose original full path matches.
        let best = items
            .iter()
            .filter(|it| it.original_parent.join(&it.name).as_path() == want)
            .max_by_key(|it| it.time_deleted)
            .cloned();
        match best {
            Some(it) => to_restore.push(it),
            None => return Err(AppError::Other(format!("Not found in the Recycle Bin: {p}"))),
        }
    }
    restore_all(to_restore).map_err(|e| AppError::Other(format!("Cannot restore: {e}")))
}

#[cfg(target_os = "macos")]
fn untrash_paths(_paths: Vec<String>) -> Result<(), AppError> {
    Err(AppError::Other(
        "Restoring from the Trash isn't supported on macOS".into(),
    ))
}

// ---------------------------------------------------------------------------
// Recycle Bin view — list / restore / purge / empty
// ---------------------------------------------------------------------------
// Windows + Linux are backed by `trash::os_limited` (with original locations,
// so restore is a true "Put Back"). macOS has no such API, so it's backed by
// the user's ~/.Trash directory directly: list reads it, purge/empty remove
// from it, and restore recovers to the Desktop (the original path isn't stored
// anywhere readable, so a real Put Back isn't possible there).

/// One item currently in the OS trash / Recycle Bin.
#[derive(Debug, Serialize, Clone)]
pub struct TrashEntry {
    /// Opaque, stable handle (the OS trash id) used to restore / purge it.
    pub id:            String,
    pub name:          String,
    /// Original absolute path it was deleted from (parent + name).
    pub original_path: String,
    /// Deletion time as a Unix timestamp in seconds (`None` when unknown).
    pub deleted_at:    Option<i64>,
}

#[cfg(not(target_os = "macos"))]
fn trash_list_blocking() -> Result<Vec<TrashEntry>, AppError> {
    use trash::os_limited::list;
    let mut items = list().map_err(|e| AppError::Other(format!("Cannot read the Recycle Bin: {e}")))?;
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
fn macos_trash_dir() -> Result<PathBuf, AppError> {
    dirs::home_dir()
        .map(|h| h.join(".Trash"))
        .ok_or_else(|| AppError::Other("No home directory".into()))
}

#[cfg(target_os = "macos")]
fn trash_list_blocking() -> Result<Vec<TrashEntry>, AppError> {
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
fn macos_trash_restore(ids: &[String]) -> Result<(), AppError> {
    let dest = dirs::desktop_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| AppError::Other("No restore destination".into()))?;
    for id in ids {
        let src = Path::new(id);
        if !src.exists() { continue; }
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| AppError::Other(format!("Invalid item: {id}")))?;
        let target = unique_dest(&dest, &name);
        std::fs::rename(src, &target)
            .map_err(|e| AppError::Other(format!("Cannot restore {name}: {e}")))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_trash_purge(ids: &[String]) -> Result<(), AppError> {
    for id in ids {
        let p = Path::new(id);
        if !p.exists() { continue; }
        let r = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
        r.map_err(|e| AppError::Other(format!("Cannot delete: {e}")))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_trash_empty() -> Result<(), AppError> {
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
#[tauri::command]
pub async fn fs_trash_list() -> Result<Vec<TrashEntry>, AppError> {
    tokio::task::spawn_blocking(trash_list_blocking)
        .await
        .map_err(|e| AppError::Other(format!("fs_trash_list task panicked: {e}")))?
}

/// Resolve the requested ids against the current trash listing. Errors when an
/// id no longer matches anything (the item was already restored / purged).
#[cfg(not(target_os = "macos"))]
fn collect_trash_items(ids: &[String]) -> Result<Vec<trash::TrashItem>, AppError> {
    use trash::os_limited::list;
    let items = list().map_err(|e| AppError::Other(format!("Cannot read the Recycle Bin: {e}")))?;
    let want: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let picked: Vec<trash::TrashItem> = items
        .into_iter()
        .filter(|it| want.contains(it.id.to_string_lossy().as_ref()))
        .collect();
    if picked.is_empty() && !ids.is_empty() {
        return Err(AppError::Other("Selected items are no longer in the Recycle Bin".into()));
    }
    Ok(picked)
}

/// Restore trashed items (by id). Windows + Linux put them back to their
/// original location; macOS recovers them to the Desktop (no Put-Back API).
#[tauri::command]
pub async fn fs_trash_restore(ids: Vec<String>) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        return tokio::task::spawn_blocking(move || macos_trash_restore(&ids))
            .await
            .map_err(|e| AppError::Other(format!("fs_trash_restore task panicked: {e}")))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        tokio::task::spawn_blocking(move || {
            let items = collect_trash_items(&ids)?;
            trash::os_limited::restore_all(items)
                .map_err(|e| AppError::Other(format!("Cannot restore: {e}")))
        })
        .await
        .map_err(|e| AppError::Other(format!("fs_trash_restore task panicked: {e}")))?
    }
}

/// Permanently delete trashed items (by id) — they leave the Recycle Bin for
/// good. Windows + Linux + macOS.
#[tauri::command]
pub async fn fs_trash_purge(ids: Vec<String>) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        return tokio::task::spawn_blocking(move || macos_trash_purge(&ids))
            .await
            .map_err(|e| AppError::Other(format!("fs_trash_purge task panicked: {e}")))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        tokio::task::spawn_blocking(move || {
            let items = collect_trash_items(&ids)?;
            trash::os_limited::purge_all(items)
                .map_err(|e| AppError::Other(format!("Cannot delete: {e}")))
        })
        .await
        .map_err(|e| AppError::Other(format!("fs_trash_purge task panicked: {e}")))?
    }
}

/// Empty the Recycle Bin entirely (permanent). Windows + Linux + macOS.
#[tauri::command]
pub async fn fs_trash_empty() -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        return tokio::task::spawn_blocking(macos_trash_empty)
            .await
            .map_err(|e| AppError::Other(format!("fs_trash_empty task panicked: {e}")))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        tokio::task::spawn_blocking(move || {
            use trash::os_limited::{list, purge_all};
            let items = list().map_err(|e| AppError::Other(format!("Cannot read the Recycle Bin: {e}")))?;
            if items.is_empty() { return Ok(()); }
            purge_all(items).map_err(|e| AppError::Other(format!("Cannot empty the Recycle Bin: {e}")))
        })
        .await
        .map_err(|e| AppError::Other(format!("fs_trash_empty task panicked: {e}")))?
    }
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

/// Open the OS terminal rooted at `path` — the folder itself, or a file's
/// parent folder. The terminal is spawned detached so it outlives Arbor.
///
/// - **Windows** → Windows Terminal (`wt.exe -d`), falling back to a classic
///   `cmd.exe` console when `wt` isn't installed.
/// - **macOS** → Terminal.app via `open -a Terminal`.
/// - **Linux/BSD** → the first available common terminal emulator, launched
///   with its working directory set to the folder.
#[tauri::command]
pub fn fs_open_terminal(path: String) -> Result<(), AppError> {
    let p = Path::new(&path);
    let dir = if p.is_dir() {
        p.to_path_buf()
    } else {
        p.parent().map(Path::to_path_buf).unwrap_or_else(|| p.to_path_buf())
    };
    if !dir.exists() {
        return Err(AppError::Other(format!("Path does not exist: {path}")));
    }
    open_terminal_at(&dir)
}

#[cfg(windows)]
fn open_terminal_at(dir: &Path) -> Result<(), AppError> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32          = 0x0000_0008;
    const CREATE_NEW_CONSOLE: u32        = 0x0000_0010;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

    let dir_str = dir.to_string_lossy().to_string();
    // Prefer Windows Terminal (its own window → DETACHED_PROCESS is fine).
    let wt = std::process::Command::new("wt.exe")
        .args(["-d", &dir_str])
        .creation_flags(DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB)
        .spawn();
    if wt.is_ok() {
        return Ok(());
    }
    // Fall back to a fresh cmd.exe console rooted at the folder. CREATE_NEW_CONSOLE
    // gives the interactive window; breakaway escapes a dev-mode parent job (with
    // the documented ERROR_ACCESS_DENIED fallback when breakaway isn't allowed).
    let build = |flags: u32| {
        std::process::Command::new("cmd.exe")
            .current_dir(dir)
            .creation_flags(flags)
            .spawn()
    };
    build(CREATE_NEW_CONSOLE | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|e| if e.raw_os_error() == Some(5) { build(CREATE_NEW_CONSOLE) } else { Err(e) })
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("Cannot open terminal: {e}")))
}

#[cfg(target_os = "macos")]
fn open_terminal_at(dir: &Path) -> Result<(), AppError> {
    std::process::Command::new("open")
        .arg("-a")
        .arg("Terminal")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("Cannot open terminal: {e}")))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_terminal_at(dir: &Path) -> Result<(), AppError> {
    use std::os::unix::process::CommandExt;
    // Common terminal emulators, probed in order. Each one starts in the working
    // directory we set on the command, so no per-terminal "cd here" flag is needed.
    const TERMINALS: &[&str] = &[
        "x-terminal-emulator", "gnome-terminal", "konsole",
        "xfce4-terminal", "alacritty", "kitty", "tilix", "xterm",
    ];
    for prog in TERMINALS {
        let spawned = std::process::Command::new(prog)
            .current_dir(dir)
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if spawned.is_ok() {
            return Ok(());
        }
    }
    Err(AppError::Other("No terminal emulator found".into()))
}

/// Expand environment-variable references and a leading `~` in a user-typed
/// path, so the address bar accepts shell-style shortcuts like `%appdata%`
/// (Windows) or `$HOME` / `~/code` (Unix). Both `%VAR%` and `$VAR` / `${VAR}`
/// syntaxes are honoured on every platform; the virtual names `appdata` /
/// `localappdata` / `home` resolve to the right OS folder everywhere, so
/// `%appdata%` works on macOS and Linux too. Unknown variables are left intact.
#[tauri::command]
pub fn fs_expand_path(path: String) -> String {
    expand_path_str(&path)
}

/// Resolve one variable name to a path value. Recognises a few cross-platform
/// virtual names before falling back to a real environment variable (exact,
/// then upper-cased so `%appdata%` matches `APPDATA`).
fn resolve_path_var(name: &str) -> Option<String> {
    let to_s = |p: PathBuf| p.to_string_lossy().into_owned();
    match name.to_ascii_lowercase().as_str() {
        "appdata"              => std::env::var("APPDATA").ok().or_else(|| dirs::config_dir().map(to_s)),
        "localappdata"         => std::env::var("LOCALAPPDATA").ok().or_else(|| dirs::data_local_dir().map(to_s)),
        "home" | "userprofile" => dirs::home_dir().map(to_s),
        _ => std::env::var(name).ok().or_else(|| std::env::var(name.to_ascii_uppercase()).ok()),
    }
}

fn expand_path_str(input: &str) -> String {
    let mut s = input.trim().to_string();
    if s.is_empty() {
        return s;
    }
    // Leading ~ → home directory (bare `~`, or `~/…` / `~\…`).
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            s = format!("{}{}", home.to_string_lossy(), &s[1..]);
        }
    }
    s = expand_percent_vars(&s);
    s = expand_dollar_vars(&s);
    s
}

/// Expand `%VAR%` tokens. A `%` with no closing `%`, or an unknown variable, is
/// left verbatim (so a stray `%` in a real filename survives).
fn expand_percent_vars(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == '%') {
                let end = i + 1 + rel;
                let name: String = chars[i + 1..end].iter().collect();
                if let Some(val) = resolve_path_var(&name) {
                    out.push_str(&val);
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Expand `$VAR` and `${VAR}` tokens. Variable names are `[A-Za-z0-9_]`.
fn expand_dollar_vars(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            let braced = chars[i + 1] == '{';
            let start = if braced { i + 2 } else { i + 1 };
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                && !(braced && chars[end] == '}')
            {
                end += 1;
            }
            let name: String = chars[start..end].iter().collect();
            let close_ok = !braced || (end < chars.len() && chars[end] == '}');
            if !name.is_empty() && close_ok {
                if let Some(val) = resolve_path_var(&name) {
                    out.push_str(&val);
                    i = if braced { end + 1 } else { end };
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
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
pub fn fs_watch_start(
    window: tauri::WebviewWindow,
    path: String,
    recursive: Option<bool>,
) -> Result<(), AppError> {
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

    let mode = if recursive.unwrap_or(false) {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    watcher
        .watch(Path::new(&path), mode)
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

// ---------------------------------------------------------------------------
// WSL distributions (Windows) — mounted under \\wsl.localhost\<distro>
// ---------------------------------------------------------------------------

/// Enumerate installed WSL distributions via `wsl.exe --list --quiet` and map
/// each to its `\\wsl.localhost\<distro>` UNC root (browsable by `fs_read_dir`
/// like any other path). `wsl.exe` prints UTF-16LE, so we decode accordingly.
/// Returns empty when WSL isn't installed (the command fails) or off-Windows.
#[cfg(windows)]
fn enumerate_wsl() -> Vec<FsRoot> {
    use crate::process_ext::NoWindowExt;
    let Ok(out) = std::process::Command::new("wsl.exe")
        .args(["--list", "--quiet"])
        .no_window()
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    // Decode UTF-16LE (skip a leading BOM if present).
    let u16s: Vec<u16> = out.stdout.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let text = String::from_utf16_lossy(&u16s);
    text.lines()
        .map(|l| l.trim().trim_matches('\u{0}').trim_matches('\u{feff}').trim())
        .filter(|n| !n.is_empty())
        .map(|name| FsRoot {
            name: name.to_string(),
            path: format!(r"\\wsl.localhost\{name}"),
            kind: "wsl".to_string(),
        })
        .collect()
}

#[cfg(not(windows))]
fn enumerate_wsl() -> Vec<FsRoot> {
    Vec::new()
}

/// List installed WSL distributions as navigable roots. Loaded once (not on the
/// removable-media poll) since spawning `wsl.exe` repeatedly would be wasteful.
#[tauri::command]
pub async fn list_wsl_distros() -> Vec<FsRoot> {
    tokio::task::spawn_blocking(enumerate_wsl)
        .await
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Recursive directory size (folder Properties) + selection info
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct DirSize {
    /// Total bytes of all files under the path (directories themselves: 0).
    pub bytes: u64,
    /// File count (excluding directories).
    pub files: u64,
    /// Sub-directory count (excluding the path itself).
    pub dirs:  u64,
}

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

/// Recursively compute the size (bytes + file/dir counts) under `path`. Used by
/// the explorer's folder Properties and the multi-selection footer. Runs on the
/// blocking pool — a deep tree can take a while, so callers show a spinner.
#[tauri::command]
pub async fn fs_dir_size(path: String) -> Result<DirSize, AppError> {
    tokio::task::spawn_blocking(move || dir_size_blocking(Path::new(&path)))
        .await
        .map_err(|e| AppError::Other(format!("fs_dir_size task panicked: {e}")))
}

/// Total size of several paths at once (folders recursed, files summed) — the
/// multi-selection footer's "N items · X total" figure.
#[tauri::command]
pub async fn fs_paths_size(paths: Vec<String>) -> Result<DirSize, AppError> {
    tokio::task::spawn_blocking(move || {
        let mut acc = DirSize { bytes: 0, files: 0, dirs: 0 };
        for p in &paths {
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
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_paths_size task panicked: {e}")))
}

// ---------------------------------------------------------------------------
// Overview dashboard — real storage stats per drive
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct DriveUsage {
    pub name:  String,
    pub path:  String,
    /// Total capacity in bytes. `None` when the platform/volume can't report it.
    pub total: Option<u64>,
    /// Free (available-to-caller) bytes. `None` when unavailable.
    pub free:  Option<u64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct OverviewStats {
    pub drives:         Vec<DriveUsage>,
    /// Sum of known drive capacities (bytes).
    pub total_capacity: u64,
    /// Sum of known free space (bytes).
    pub total_free:     u64,
}

#[cfg(windows)]
fn disk_free_total(path: &str) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_avail: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_avail, &mut total, &mut total_free) };
    if ok != 0 { Some((free_avail, total)) } else { None }
}

#[cfg(not(windows))]
fn disk_free_total(path: &str) -> Option<(u64, u64)> {
    // No std API for free space, so shell out to `df` (present on both Linux and
    // macOS) rather than pull in a new crate. `-P` forces single-line POSIX
    // output, `-k` reports 1024-byte blocks. Columns:
    //   Filesystem  1024-blocks  Used  Available  Capacity  Mounted-on
    let out = std::process::Command::new("df")
        .args(["-k", "-P", path])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?; // skip the header row
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 4 {
        return None;
    }
    let total_kb: u64 = cols[1].parse().ok()?;
    let avail_kb: u64 = cols[3].parse().ok()?;
    Some((avail_kb.saturating_mul(1024), total_kb.saturating_mul(1024)))
}

/// Real Overview dashboard stats: capacity / free space per drive (Windows;
/// `None` on platforms without a std API). The frontend renders the per-drive
/// usage bars and the aggregate capacity / free / used figures from this.
#[tauri::command]
pub async fn fs_overview_stats() -> OverviewStats {
    tokio::task::spawn_blocking(|| {
        let drives = list_fs_roots_blocking()
            .into_iter()
            .filter(|r| r.kind == "drive")
            .map(|r| {
                let (free, total) = match disk_free_total(&r.path) {
                    Some((f, t)) => (Some(f), Some(t)),
                    None => (None, None),
                };
                DriveUsage { name: r.name, path: r.path, total, free }
            })
            .collect::<Vec<_>>();
        let total_capacity = drives.iter().filter_map(|d| d.total).sum();
        let total_free     = drives.iter().filter_map(|d| d.free).sum();
        OverviewStats { drives, total_capacity, total_free }
    })
    .await
    .unwrap_or(OverviewStats { drives: Vec::new(), total_capacity: 0, total_free: 0 })
}
