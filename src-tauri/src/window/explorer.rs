//! Dedicated File Explorer window + its OS-global activation shortcut.
//!
//! `Ctrl+Shift+E` (registered system-wide via `tauri-plugin-global-shortcut`,
//! so it fires even when Arbor isn't focused) opens a standalone, Arbor-styled
//! window that hosts ONLY the built-in file explorer — not the full app.
//!
//! The window loads the same `index.html` as the main window; the frontend
//! root (`src/routes/+page.svelte`) branches on the window label
//! ([`EXPLORER_WINDOW_LABEL`]) to mount the standalone explorer shell
//! (`ExplorerWindow.svelte`) instead of `AppShell`. This avoids a second
//! SvelteKit route / prerender entirely — both windows share one entry point.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::Shortcut;

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// A pending "reveal this path" request handed to a freshly-opened explorer
/// window. The frontend pulls it on mount (via [`take_explorer_reveal`]) once
/// its listeners are wired — avoiding the emit-before-listen race a new window
/// would otherwise hit. Already-open windows get the same payload by event.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RevealPayload {
    /// Folder the explorer should navigate to.
    pub dir: String,
    /// File name to select inside `dir` once it loads (None ⇒ just open the
    /// folder, no selection — used for "open folder" as opposed to "reveal").
    pub select: Option<String>,
}

/// Per-window pending reveals, keyed by window label. Populated when a reveal
/// spawns a NEW explorer window; drained by the window's frontend on mount.
#[derive(Default)]
pub struct PendingReveals(pub Mutex<HashMap<String, RevealPayload>>);

/// Window label for the dedicated explorer window. The frontend reads
/// `getCurrentWindow().label` and matches this to switch into explorer mode.
pub const EXPLORER_WINDOW_LABEL: &str = "explorer";

/// Parse a Tauri accelerator string (e.g. `"Ctrl+Shift+E"`) into a `Shortcut`.
/// Returns `None` for an empty or unparseable string.
fn parse_accel(accel: &str) -> Option<Shortcut> {
    let a = accel.trim();
    if a.is_empty() { return None; }
    Shortcut::from_str(a).ok()
}

/// The currently-configured explorer global shortcut, or `None` when the
/// feature is disabled or the accelerator is unparseable. Read from disk so the
/// global-shortcut press handler (which runs off the UI thread) and the
/// register/reconcile paths share one source of truth.
pub fn current_explorer_shortcut() -> Option<Shortcut> {
    let cfg = crate::config::app_config::load().ok()?;
    if !cfg.explorer.global_shortcut { return None; }
    parse_accel(&cfg.explorer.global_shortcut_accel)
}

/// Open the dedicated explorer window, or focus it if it already exists
/// (single-instance: one explorer window, re-summoned rather than duplicated).
///
/// Both entry points (the global-shortcut handler and the `open_explorer_window`
/// IPC command) run on **background** threads, but WebView2 window creation must
/// happen on the **main/UI** thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    ensure_backend(app);
    super::dispatch_to_main(app, "explorer", create_or_focus);
}

/// Bring up `sitta-be` (the file-explorer backend) off the main thread, so the
/// spawn's blocking first-`Hello` read never stalls the UI thread. Idempotent — a
/// no-op once the backend is attached. Called from every explorer entry point
/// (command, global shortcut, tray, reveal) so the backend is coming up while the
/// window boots. Nothing routes to `sitta` yet (Onda 1), so a missing/slow backend
/// is harmless; the window opens regardless.
fn ensure_backend(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || crate::ipc::ensure_sitta_be(&app));
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
///
/// Behaviour depends on `explorer.always_new_window`: when false (default) a
/// single explorer window is reused (re-summoning focuses it); when true a new
/// window is opened every time, each with a unique `explorer-N` label.
fn create_or_focus(app: &AppHandle) {
    let always_new = crate::config::app_config::load()
        .map(|c| c.explorer.always_new_window)
        .unwrap_or(false);

    if !always_new {
        if let Some(w) = app.get_webview_window(EXPLORER_WINDOW_LABEL) {
            show_and_focus(&w);
            super::emit_product_state(app, "sitta", true);
            return;
        }
    }

    let label = next_explorer_label(app);
    build_explorer_window(app, &label);
    super::emit_product_state(app, "sitta", true);
}

/// Build a frameless explorer window with the given label.
///
/// `WebviewUrl::default()` resolves to the app's index (`index.html`) — the
/// same entry the main window uses — so the load path is identical in dev
/// (Vite) and packaged builds. Frameless to match Arbor's main window; the
/// standalone shell paints its own titlebar + WindowControls.
fn build_explorer_window(app: &AppHandle, label: &str) {
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .title("File Explorer — Arbor")
        .inner_size(1100.0, 720.0)
        .min_inner_size(720.0, 460.0)
        .shadow(true)
        .center()
        // Build HIDDEN and reveal once the shell has painted (window_ready) — an
        // opaque WebView2 window would otherwise flash its white default page during
        // load. See super::window_ready / arm_ready_reveal.
        .visible(false)
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS);
    // Native traffic lights on macOS, frameless elsewhere (see super::native_titlebar).
    let res = super::native_titlebar(builder).build();

    match res {
        // The explorer window IS Sitta — the label predates the product name.
        Ok(w) => {
            super::apply_product_icon(&w, "sitta");
            super::arm_ready_reveal(app, label)
        }
        Err(e) => tracing::error!("failed to open explorer window: {e}"),
    }
}

/// Open the explorer at a folder (and optionally select a file), reusing the
/// existing window when one-window mode is on. Mirrors [`open_or_focus`]'s
/// thread hop (WebView2 window ops must run on the main thread).
fn open_or_focus_reveal(app: &AppHandle, payload: RevealPayload) {
    ensure_backend(app);
    super::dispatch_to_main(app, "explorer reveal", move |a| create_or_focus_reveal(a, payload));
}

/// Main-thread body of [`open_or_focus_reveal`].
///
/// One-window mode (default): focus the single explorer window and hand it the
/// reveal by event — it reuses an open tab for that folder or opens a new one.
/// New-window mode: spawn a fresh window with the reveal stashed for the
/// frontend to pull on mount (no cross-window tab dedup in this mode).
fn create_or_focus_reveal(app: &AppHandle, payload: RevealPayload) {
    let always_new = crate::config::app_config::load()
        .map(|c| c.explorer.always_new_window)
        .unwrap_or(false);

    if !always_new {
        if let Some(w) = app.get_webview_window(EXPLORER_WINDOW_LABEL) {
            show_and_focus(&w);
            let _ = w.emit("arbor://explorer-reveal", &payload);
            super::emit_product_state(app, "sitta", true);
            return;
        }
    }

    let label = next_explorer_label(app);
    if let Some(state) = app.try_state::<PendingReveals>() {
        if let Ok(mut map) = state.0.lock() {
            map.insert(label.clone(), payload);
        }
    }
    build_explorer_window(app, &label);
    super::emit_product_state(app, "sitta", true);
}

/// Resolve a raw path + `reveal` flag into a [`RevealPayload`]: when revealing a
/// file, navigate to its parent and select it; otherwise open the folder
/// itself with no selection. The file/dir check is a single `stat`.
fn resolve_reveal(path: &str, reveal: bool) -> RevealPayload {
    let p = std::path::Path::new(path);
    if reveal && p.is_file() {
        let dir = p
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        let select = p.file_name().map(|n| n.to_string_lossy().into_owned());
        RevealPayload { dir, select }
    } else {
        RevealPayload { dir: path.to_string(), select: None }
    }
}

/// IPC entry point for the app-wide "Open / Reveal in File Explorer" actions
/// when the user routes them to the built-in explorer
/// (`explorer.reveal_in_builtin`). `reveal = true` selects the file inside its
/// folder; `reveal = false` just opens the folder. Async for the same
/// main-thread-hop reason as [`open_explorer_window`].
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn reveal_in_explorer(app: AppHandle, path: String, reveal: bool) {
    let payload = resolve_reveal(&path, reveal);
    open_or_focus_reveal(&app, payload);
}

/// Route an app/plugin "open in file explorer" request (`arbor.ui.open_path`,
/// the `__open_path` reverse-channel handler, and `TauriAppCtx::open_path`)
/// through the user's OS-vs-built-in preference (`explorer.reveal_in_builtin`).
///
/// Regardless of target, a FILE is revealed inside its containing folder (the
/// folder is opened with the file selected) and a FOLDER is opened as the
/// listing — so both routes behave the same way. When `reveal_in_builtin` is
/// on, the path lands in Arbor's built-in explorer window; otherwise it goes to
/// the OS file manager (Explorer / Finder / xdg-open).
///
/// Never blocks: the built-in route hops to the main thread internally, and the
/// OS route is a fire-and-forget opener call. Errors from the OS opener are
/// surfaced to the caller (the built-in route is best-effort).
pub fn reveal_path(app: &AppHandle, path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("reveal_path: path cannot be empty".to_string());
    }

    let to_builtin = crate::config::app_config::load()
        .map(|c| c.explorer.reveal_in_builtin)
        .unwrap_or(false);

    if to_builtin {
        // Built-in explorer: reveal (select the file / open the folder).
        open_or_focus_reveal(app, resolve_reveal(path, true));
        return Ok(());
    }

    // OS file manager: reveal a file inside its folder, or open a folder.
    use tauri_plugin_opener::OpenerExt;
    let p = std::path::Path::new(path);
    if p.is_file() {
        app.opener()
            .reveal_item_in_dir(path)
            .map_err(|e| format!("Cannot reveal: {e}"))
    } else {
        app.opener()
            .open_path(path, None::<&str>)
            .map_err(|e| format!("Cannot open: {e}"))
    }
}

/// Drain the pending reveal for a window label (called by a freshly-opened
/// explorer window's frontend on mount). Returns `None` when there's nothing
/// pending (normal global-shortcut / palette opens).
#[tauri::command]
pub fn take_explorer_reveal(state: State<'_, PendingReveals>, label: String) -> Option<RevealPayload> {
    state.0.lock().ok()?.remove(&label)
}

/// Pick a free window label: the canonical `explorer` when available, otherwise
/// the first free `explorer-N`. Labels are reused once a window closes. The
/// frontend (`+page.svelte`) treats any `explorer`/`explorer-*` label as an
/// explorer window.
fn next_explorer_label(app: &AppHandle) -> String {
    if app.get_webview_window(EXPLORER_WINDOW_LABEL).is_none() {
        return EXPLORER_WINDOW_LABEL.to_string();
    }
    for i in 2..1000 {
        let label = format!("{EXPLORER_WINDOW_LABEL}-{i}");
        if app.get_webview_window(&label).is_none() {
            return label;
        }
    }
    // Absurd fallback (1000 explorer windows open) — reuse the canonical label.
    EXPLORER_WINDOW_LABEL.to_string()
}

/// IPC entry point so the in-app Command Palette ("Open File Explorer in New
/// Window") can summon the same window the global shortcut does.
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**,
/// and dispatching WebView2 window creation via `run_on_main_thread` from the
/// main thread (while it's blocked inside this command) leaves the new window
/// with an uninitialised webview — a blank window with no devtools. As an async
/// command it runs on the async runtime (a background thread), so the
/// `run_on_main_thread` hop in `open_or_focus` behaves exactly like the
/// global-shortcut handler (which also runs off the main thread).
#[tauri::command]
#[allow(clippy::unused_async)] // async is load-bearing here: it moves the
// handler off the main thread (see doc comment) — there's nothing to await.
pub async fn open_explorer_window(app: AppHandle) {
    open_or_focus(&app);
}

/// Register the configured explorer shortcut at startup (no-op when the feature
/// is off or the accelerator is invalid). Failures are logged, not fatal.
#[cfg(desktop)]
pub fn register_configured(app: &AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if let Some(sc) = current_explorer_shortcut() {
        if let Err(e) = app.global_shortcut().register(sc) {
            tracing::warn!("failed to register explorer global shortcut: {e}");
        }
    }
}

/// Reconcile the OS-global shortcut when the explorer config changes: unregister
/// the previously-active combo and register the new one. A combo is "active"
/// only when the feature is enabled. Returns an error (surfaced to the UI) when
/// the new accelerator is invalid or already claimed by another app, so the
/// settings UI can revert and toast.
#[cfg(desktop)]
pub fn reconcile_global_shortcut(
    app: &AppHandle,
    old: &crate::config::app_config::ExplorerConfig,
    new: &crate::config::app_config::ExplorerConfig,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    // Compare by (enabled, accel) so identical settings short-circuit.
    let old_active = old.global_shortcut.then(|| old.global_shortcut_accel.trim().to_string());
    let new_active = new.global_shortcut.then(|| new.global_shortcut_accel.trim().to_string());
    if old_active == new_active { return Ok(()); }

    let gs = app.global_shortcut();
    if let Some(a) = old_active {
        if let Some(sc) = parse_accel(&a) { let _ = gs.unregister(sc); }
    }
    if let Some(a) = new_active {
        match parse_accel(&a) {
            Some(sc) => gs.register(sc).map_err(|e| format!("Couldn't register {a}: {e}"))?,
            None => return Err(format!("Invalid shortcut: {a}")),
        }
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
//  Cross-window clipboard (copy / cut / paste between Arbor explorer windows)
// ───────────────────────────────────────────────────────────────────────────
//
// Each explorer window is its own JS context, so a per-window in-memory
// clipboard can't be pasted into another window. The OS clipboard's file
// formats (CF_HDROP & friends) aren't reachable from the WebView either. Since
// every window lives in ONE process, we keep the clipboard in shared Tauri
// state and broadcast changes so every open explorer mirrors it — copy in one
// window, paste in another. The actual copy/move is a plain `fs_copy`/`fs_move`
// the pasting window runs; this state only carries the intent (paths + op).

/// The shared clipboard payload. `op` is `"copy"` or `"cut"`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClipData {
    pub op: String,
    pub paths: Vec<String>,
}

/// Process-wide explorer clipboard, shared by every window via Tauri state.
#[derive(Default)]
pub struct ExplorerClipboard(pub Mutex<Option<ClipData>>);

/// Store `op`+`paths` as the active clipboard and broadcast the new contents to
/// every window (so each explorer's "cut" dimming + footer chip stay in sync).
#[tauri::command]
pub fn explorer_clip_set(app: AppHandle, state: State<'_, ExplorerClipboard>, op: String, paths: Vec<String>) {
    let data = ClipData { op, paths };
    if let Ok(mut g) = state.0.lock() { *g = Some(data.clone()); }
    let _ = app.emit("arbor://explorer-clip-changed", Some(data));
}

/// Read the current clipboard (an explorer window seeds its local mirror with
/// this on mount so it knows about a copy made before it opened).
#[tauri::command]
pub fn explorer_clip_get(state: State<'_, ExplorerClipboard>) -> Option<ClipData> {
    state.0.lock().ok().and_then(|g| g.clone())
}

/// Clear the clipboard (after a cut→paste move completes) and broadcast the
/// cleared state so every window drops its "cut" dimming.
#[tauri::command]
pub fn explorer_clip_clear(app: AppHandle, state: State<'_, ExplorerClipboard>) {
    if let Ok(mut g) = state.0.lock() { *g = None; }
    let _ = app.emit("arbor://explorer-clip-changed", None::<ClipData>);
}

// ───────────────────────────────────────────────────────────────────────────
//  Drag & drop out of, and between, explorer windows
// ───────────────────────────────────────────────────────────────────────────
//
// The drag itself is the OS's (`tauri-plugin-drag`, started from the front end), so it carries the
// real files and can end on any window or application. What the OS hands a webview that is dropped
// on is a list of `File` objects without paths, so the paths of the drag in flight are kept here:
// the explorer window that receives the drop asks for them, which is how a move between two
// explorer windows knows what to move.

/// The paths of the drag that started in an explorer window. Overwritten by the next drag; cleared
/// shortly after one ends (the receiving window may still be reading it as the drag finishes).
#[derive(Default)]
pub struct ExplorerDrag(pub Mutex<Vec<String>>);

/// Remember the paths a drag is carrying.
#[tauri::command]
pub fn explorer_drag_set(state: State<'_, ExplorerDrag>, paths: Vec<String>) {
    if let Ok(mut g) = state.0.lock() { *g = paths; }
}

/// The paths of the drag in flight (empty when it did not start in an explorer window).
#[tauri::command]
pub fn explorer_drag_get(state: State<'_, ExplorerDrag>) -> Vec<String> {
    state.0.lock().map(|g| g.clone()).unwrap_or_default()
}

/// Forget them.
#[tauri::command]
pub fn explorer_drag_clear(state: State<'_, ExplorerDrag>) {
    if let Ok(mut g) = state.0.lock() { g.clear(); }
}

/// The picture the OS shows under the cursor for a drag, when the front end has none of its own:
/// the app icon, written once into the cache folder because the drag plugin takes a file path.
#[tauri::command]
pub fn drag_icon_path(app: AppHandle) -> Result<String, String> {
    let file = drag_cache_dir(&app)?.join("drag-icon.png");
    if !file.is_file() {
        std::fs::write(&file, include_bytes!("../../icons/32x32.png")).map_err(|e| e.to_string())?;
    }
    Ok(file.to_string_lossy().into_owned())
}

/// Write the PNG the front end drew for this drag (its label on a pill) and return its path.
#[tauri::command]
pub fn drag_icon_write(app: AppHandle, png: Vec<u8>) -> Result<String, String> {
    let file = drag_cache_dir(&app)?.join("drag-label.png");
    std::fs::write(&file, png).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().into_owned())
}

fn drag_cache_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}
