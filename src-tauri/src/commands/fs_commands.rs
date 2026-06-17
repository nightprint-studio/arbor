//! Tauri command layer for the OS / Tauri shell-integration FS commands that are
//! *not* pure filesystem I/O.
//!
//! The pure FS logic (read/write/list/search/trash/zip, roots, sizes, path
//! expansion) and the progress-emitting copy / move / duplicate / cancel ops
//! have moved to the platform broker ([`crate::ipc::platform::fs`]) — the copy
//! ops now stream `arbor://fs-op-progress` through the backend event sink. What
//! stays here shells out to the OS for an integration the pure layer can't
//! provide:
//!
//! - **OS shell-integration** — open-with-default, reveal-in-file-manager,
//!   open-terminal, the native Properties dialog, set-wallpaper, native icons,
//!   and the per-window watcher.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::error::AppError;

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
