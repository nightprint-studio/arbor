//! Tauri command layer over the pure `arbor-fs` crate.
//!
//! The filesystem logic (read/write/copy/move/delete/list/search/trash/zip,
//! roots, sizes, path expansion) lives in `arbor-fs`; these commands are thin
//! wrappers that map `FsError` → [`AppError`], drive the blocking calls on
//! `spawn_blocking`, and supply the Tauri-specific glue the pure layer takes via
//! traits: the progress [`EmitSink`] (emits `arbor://fs-op-progress`) and the
//! op-id → [`CancelToken`] registry that backs [`fs_cancel_op`].
//!
//! What stays here is the OS / Tauri shell-integration that is *not* pure FS
//! I/O: open-with-default, reveal-in-file-manager, open-terminal, the native
//! Properties dialog, set-wallpaper, native icons, and the per-window watcher.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;
use tauri::Emitter;

use crate::error::AppError;
use arbor_fs::prelude::{
    copy, mutate, pathexpand, read, roots, size, trash, zip, CancelToken, DirSize, FsEntry, FsRoot,
    NoopSink, OverviewStats, ProgressSink, RenamePair, TrashEntry,
};

// ---------------------------------------------------------------------------
// Mutating filesystem operations
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn fs_create_dir(path: String) -> Result<(), AppError> {
    Ok(mutate::create_dir(&path)?)
}

#[tauri::command]
pub fn fs_create_file(path: String) -> Result<(), AppError> {
    Ok(mutate::create_file(&path)?)
}

#[tauri::command]
pub fn fs_rename(old_path: String, new_path: String) -> Result<(), AppError> {
    Ok(mutate::rename(&old_path, &new_path)?)
}

#[tauri::command]
pub fn fs_rename_many(pairs: Vec<RenamePair>) -> Result<Vec<String>, AppError> {
    Ok(mutate::rename_many(&pairs)?)
}

#[tauri::command]
pub fn fs_write_text_file(path: String, content: String) -> Result<(), AppError> {
    Ok(mutate::write_text(&path, &content)?)
}

#[tauri::command]
pub fn fs_read_text_file(path: String) -> Result<String, AppError> {
    Ok(read::read_text(&path)?)
}

#[tauri::command]
pub fn fs_delete(path: String) -> Result<(), AppError> {
    Ok(mutate::delete(&path)?)
}

// ---------------------------------------------------------------------------
// Listing + search (blocking pool — a slow drive can't stall the IPC runtime)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fs_read_dir(path: String, show_hidden: Option<bool>) -> Result<Vec<FsEntry>, AppError> {
    let show_hidden = show_hidden.unwrap_or(false);
    let entries = tokio::task::spawn_blocking(move || read::read_dir(&path, show_hidden))
        .await
        .map_err(|e| AppError::Other(format!("fs_read_dir task panicked: {e}")))??;
    Ok(entries)
}

#[tauri::command]
pub async fn fs_search(
    root: String,
    query: String,
    show_hidden: Option<bool>,
    limit: Option<usize>,
) -> Result<Vec<FsEntry>, AppError> {
    let show_hidden = show_hidden.unwrap_or(false);
    let cap = limit.unwrap_or(5000);
    let out = tokio::task::spawn_blocking(move || read::search(&root, &query, show_hidden, cap))
        .await
        .map_err(|e| AppError::Other(format!("fs_search task panicked: {e}")))??;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Copy / move / duplicate — progress + cooperative cancellation
// ---------------------------------------------------------------------------
// A per-`op_id` cancel token lets the UI abort a running op; a throttled
// `arbor://fs-op-progress` event drives the explorer's progress bar. The pure
// copy/move/duplicate live in `arbor-fs`; here we own the registry and the sink.

/// Process-wide registry of cancel tokens keyed by op_id.
fn cancel_registry() -> &'static Mutex<HashMap<String, CancelToken>> {
    static REG: OnceLock<Mutex<HashMap<String, CancelToken>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_op(op_id: &str) -> CancelToken {
    let token = CancelToken::new();
    if let Ok(mut m) = cancel_registry().lock() {
        m.insert(op_id.to_string(), token.clone());
    }
    token
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
        if let Some(token) = m.get(&op_id) {
            token.cancel();
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

/// Progress sink that emits the throttled `arbor://fs-op-progress` event. The
/// totals arrive in `start`; per-file frames are throttled to ~80ms, while the
/// initial (0%) and final (100%) frames are forced so the bar appears and
/// completes.
struct EmitSink {
    app:         tauri::AppHandle,
    op_id:       String,
    kind:        &'static str,
    total_files: u64,
    total_bytes: u64,
    last_emit:   Instant,
}

impl EmitSink {
    fn emit(&mut self, done_files: u64, done_bytes: u64, current: &str, force: bool) {
        if !force && self.last_emit.elapsed().as_millis() < 80 {
            return;
        }
        self.last_emit = Instant::now();
        let _ = self.app.emit("arbor://fs-op-progress", FsOpProgress {
            op_id:       self.op_id.clone(),
            kind:        self.kind.to_string(),
            done_files,
            total_files: self.total_files,
            done_bytes,
            total_bytes: self.total_bytes,
            current:     current.to_string(),
        });
    }
}

impl ProgressSink for EmitSink {
    fn start(&mut self, total_files: u64, total_bytes: u64) {
        self.total_files = total_files;
        self.total_bytes = total_bytes;
        self.emit(0, 0, "", true);
    }
    fn file_done(&mut self, done_files: u64, done_bytes: u64, current: &str) {
        self.emit(done_files, done_bytes, current, false);
    }
    fn finish(&mut self, done_files: u64, done_bytes: u64) {
        self.emit(done_files, done_bytes, "", true);
    }
}

/// Run an op closure with the right progress sink: an [`EmitSink`] when the
/// caller supplied an `op_id` (live progress), or a no-op sink otherwise.
fn run_op<F>(
    app: &tauri::AppHandle,
    op_id: &Option<String>,
    kind: &'static str,
    f: F,
) -> Result<Vec<String>, AppError>
where
    F: FnOnce(&mut dyn ProgressSink) -> arbor_fs::prelude::Result<Vec<String>>,
{
    let res = match op_id {
        Some(id) => {
            let mut sink = EmitSink {
                app: app.clone(),
                op_id: id.clone(),
                kind,
                total_files: 0,
                total_bytes: 0,
                last_emit: Instant::now(),
            };
            f(&mut sink)
        }
        None => f(&mut NoopSink),
    };
    res.map_err(AppError::from)
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
        let cancel = match &op_id {
            Some(id) => register_op(id),
            None => CancelToken::new(),
        };
        let out = run_op(&app, &op_id, "copy", |sink| {
            copy::copy(&sources, &dest_dir, overwrite, sink, &cancel)
        });
        if let Some(id) = &op_id {
            unregister_op(id);
        }
        out
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
        let cancel = match &op_id {
            Some(id) => register_op(id),
            None => CancelToken::new(),
        };
        let out = run_op(&app, &op_id, "move", |sink| {
            copy::move_(&sources, &dest_dir, overwrite, sink, &cancel)
        });
        if let Some(id) = &op_id {
            unregister_op(id);
        }
        out
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_move task panicked: {e}")))?
}

/// Duplicate each of `paths` in place (same parent folder), Explorer-style:
/// `report.pdf` → `report (2).pdf`, a second time → `report (3).pdf`. Returns
/// the created paths. Progress/cancel work exactly like `fs_copy`.
#[tauri::command]
pub async fn fs_duplicate(
    app: tauri::AppHandle,
    paths: Vec<String>,
    op_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(move || {
        let cancel = match &op_id {
            Some(id) => register_op(id),
            None => CancelToken::new(),
        };
        let out = run_op(&app, &op_id, "duplicate", |sink| {
            copy::duplicate(&paths, sink, &cancel)
        });
        if let Some(id) = &op_id {
            unregister_op(id);
        }
        out
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_duplicate task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Delete — trash (default) vs permanent (Shift+Delete) + Recycle Bin view
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fs_trash(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || trash::trash(&paths))
        .await
        .map_err(|e| AppError::Other(format!("fs_trash task panicked: {e}")))??;
    Ok(())
}

#[tauri::command]
pub async fn fs_delete_many(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || mutate::delete_many(&paths))
        .await
        .map_err(|e| AppError::Other(format!("fs_delete_many task panicked: {e}")))??;
    Ok(())
}

#[tauri::command]
pub async fn fs_untrash(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || trash::untrash(&paths))
        .await
        .map_err(|e| AppError::Other(format!("fs_untrash task panicked: {e}")))??;
    Ok(())
}

#[tauri::command]
pub async fn fs_trash_list() -> Result<Vec<TrashEntry>, AppError> {
    let out = tokio::task::spawn_blocking(trash::trash_list)
        .await
        .map_err(|e| AppError::Other(format!("fs_trash_list task panicked: {e}")))??;
    Ok(out)
}

#[tauri::command]
pub async fn fs_trash_restore(ids: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || trash::trash_restore(&ids))
        .await
        .map_err(|e| AppError::Other(format!("fs_trash_restore task panicked: {e}")))??;
    Ok(())
}

#[tauri::command]
pub async fn fs_trash_purge(ids: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || trash::trash_purge(&ids))
        .await
        .map_err(|e| AppError::Other(format!("fs_trash_purge task panicked: {e}")))??;
    Ok(())
}

#[tauri::command]
pub async fn fs_trash_empty() -> Result<(), AppError> {
    tokio::task::spawn_blocking(trash::trash_empty)
        .await
        .map_err(|e| AppError::Other(format!("fs_trash_empty task panicked: {e}")))??;
    Ok(())
}

// ---------------------------------------------------------------------------
// Compress / extract — ZIP archives
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fs_zip(
    sources: Vec<String>,
    dest_dir: String,
    archive_name: String,
) -> Result<String, AppError> {
    let out = tokio::task::spawn_blocking(move || zip::zip(&sources, &dest_dir, &archive_name))
        .await
        .map_err(|e| AppError::Other(format!("fs_zip task panicked: {e}")))??;
    Ok(out)
}

#[tauri::command]
pub async fn fs_unzip(archive: String, dest_dir: Option<String>) -> Result<String, AppError> {
    let out = tokio::task::spawn_blocking(move || zip::unzip(&archive, dest_dir))
        .await
        .map_err(|e| AppError::Other(format!("fs_unzip task panicked: {e}")))??;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Recursive sizes + quick-access roots + WSL + Overview stats
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fs_dir_size(path: String) -> Result<DirSize, AppError> {
    tokio::task::spawn_blocking(move || size::dir_size(&path))
        .await
        .map_err(|e| AppError::Other(format!("fs_dir_size task panicked: {e}")))
}

#[tauri::command]
pub async fn fs_paths_size(paths: Vec<String>) -> Result<DirSize, AppError> {
    tokio::task::spawn_blocking(move || size::paths_size(&paths))
        .await
        .map_err(|e| AppError::Other(format!("fs_paths_size task panicked: {e}")))
}

/// Return filesystem quick-access roots (common user dirs + drives / `/`).
/// Runs on the blocking pool because `dirs::*_dir()` + `Path::exists()` can
/// touch the filesystem.
#[tauri::command]
pub async fn list_fs_roots() -> Vec<FsRoot> {
    tokio::task::spawn_blocking(roots::list_roots)
        .await
        .unwrap_or_default()
}

/// List installed WSL distributions as navigable roots. Loaded once (not on the
/// removable-media poll) since spawning `wsl.exe` repeatedly would be wasteful.
#[tauri::command]
pub async fn list_wsl_distros() -> Vec<FsRoot> {
    tokio::task::spawn_blocking(roots::list_wsl_distros)
        .await
        .unwrap_or_default()
}

/// Real Overview dashboard stats: capacity / free space per drive.
#[tauri::command]
pub async fn fs_overview_stats() -> OverviewStats {
    tokio::task::spawn_blocking(roots::overview_stats)
        .await
        .unwrap_or(OverviewStats { drives: Vec::new(), total_capacity: 0, total_free: 0 })
}

/// Expand environment-variable references and a leading `~` in a user-typed
/// path (`%appdata%`, `$HOME`, `~/code`, …). Unknown variables are left intact.
#[tauri::command]
pub fn fs_expand_path(path: String) -> String {
    pathexpand::expand_path(&path)
}

// ===========================================================================
// OS / Tauri shell-integration — stays in the shell (not pure FS I/O).
// ===========================================================================

// --- Open / reveal via the OS (default app, file manager) ------------------

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

// --- Native Properties dialog ----------------------------------------------

/// Open the OS-native file/folder **Properties** dialog for `path`.
///
/// - **Windows** → `SHObjectProperties` property sheet, on a detached thread.
/// - **macOS** → Finder *Get Info* window via AppleScript (`osascript`).
/// - **Linux/BSD** → freedesktop `FileManager1.ShowItemProperties` over D-Bus.
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
/// the UI thread.
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

/// Percent-encode an absolute path into a `file://` URI for the FileManager1
/// D-Bus call (keeps `/`, encodes spaces and other reserved bytes).
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

// --- Set desktop wallpaper --------------------------------------------------

/// Set `path` (an image file) as the desktop wallpaper.
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

// --- Native system icons ----------------------------------------------------
// The real shell/desktop icon for a file extension, returned as a PNG data-URI
// the explorer renders in an <img>. Backed by the `systemicons` crate. Folders
// are NOT covered (the crate resolves icons by a `FILE_ATTRIBUTE_NORMAL`
// query), so the explorer keeps its themed folder icon.

/// Native system icon for `query`, as a `data:image/png;base64,…` URI.
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

// --- Filesystem watcher — live updates for the explorer's current directory --
// One watcher PER WINDOW, keyed by window label: every explorer window watches
// its own current directory independently. The change signal is emitted to the
// originating window ALONE (`emit_to(label)`) — never broadcast — so one
// window's change can't make every other explorer window refetch + recompute
// git status. Coalescing happens on the frontend; the payload is empty.

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
    // Source-side throttle: an active directory makes the OS watcher fire
    // hundreds of events per second. Collapse bursts to at most one signal per
    // interval; the frontend's own 200 ms debounce coalesces the rest.
    let mut last_emit: Option<std::time::Instant> = None;
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let now = std::time::Instant::now();
            let due = last_emit.map_or(true, |t| now.duration_since(t) >= std::time::Duration::from_millis(120));
            if !due { return; }
            last_emit = Some(now);
            // Target the owning window only — broadcasting would refresh every
            // open explorer window (each doing a full git recompute).
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
