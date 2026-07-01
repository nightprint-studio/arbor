//! Native window lifecycles for Arbor's standalone surfaces.
//!
//! Arbor is a single OS process that drives several **separate top-level
//! windows**, each a frameless WebView2 loading the same `index.html`; the
//! frontend root (`src/routes/+page.svelte`) branches on the window label to
//! mount the right shell. One module per window:
//!
//! - [`explorer`] — the dedicated File Explorer (`explorer` / `explorer-N`),
//!   plus its OS-global shortcut, cross-window clipboard and drag overlay.
//! - [`merula`] — the music live-coding DAW shell (`merula`).
//! - [`corvus`] — the Git product window (`corvus`). Today the Git UI also
//!   loads in `main`; this is the seed of the launcher split, where `main`
//!   becomes the launcher and Corvus opens as a product window.
//! - [`launcher`] — the JetBrains-Toolbox-like launcher (`launcher`).
//!   Scaffolding: backend lifecycle is ready; the frontend `LauncherShell`
//!   is still to come.
//!
//! [`events`] holds the shared `on_window_event` handler.
//!
//! Common WebView2 plumbing lives here so every window stays in sync — most
//! importantly [`WEBVIEW_BROWSER_ARGS`], which **must** match across every
//! webview in the process (and the `main` window's `additionalBrowserArgs` in
//! `tauri.conf.json`).

pub mod corvus;
pub mod events;
pub mod explorer;
pub mod launcher;
pub mod merula;
pub mod placement;

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// WebView2 additional browser args, shared by **every** Arbor window. Every
/// WebView2 instance in the process shares one user-data-folder + environment,
/// so creating a second webview with *different* env options fails with
/// `HRESULT 0x8007139F` (ERROR_INVALID_STATE). Must also match the `main`
/// window's `additionalBrowserArgs` in `tauri.conf.json`.
pub const WEBVIEW_BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,InterestFeedContentSuggestions,WebRTC,AutofillServerCommunication";

/// Bring a window to the foreground: undo a minimize, show it, take focus. The
/// idempotent three-step every "focus the existing window" path repeats.
pub fn show_and_focus(w: &WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

// ───────────────────────────────────────────────────────────────────────────
//  Product window lifecycle → launcher running-state
// ───────────────────────────────────────────────────────────────────────────
//
// The launcher (the `main` window) draws each Canopy product as a node that
// lights up "In esecuzione" while its window is open. Product windows open/close
// independently (their own labels), so the launcher can't know their state from
// its own JS — the shell tells it: every open/focus emits `running: true`, and
// the last window of a product closing emits `running: false`. The launcher also
// seeds itself once on mount via [`list_running_products`] (covers windows that
// were already open before its listener was wired).

/// Map a native window label to the Canopy product id it belongs to, if any.
/// `corvus` → Corvus, `explorer`/`explorer-N` → Sitta, `merula`/`merula-N` →
/// Merula. Anything else (launcher, drag-overlay, …) is not a product window.
pub fn product_id_for_label(label: &str) -> Option<&'static str> {
    if label == corvus::CORVUS_WINDOW_LABEL || label.starts_with("corvus-") {
        Some("corvus")
    } else if label == explorer::EXPLORER_WINDOW_LABEL || label.starts_with("explorer-") {
        Some("sitta")
    } else if label == merula::MERULA_WINDOW_LABEL || label.starts_with("merula-") {
        Some("merula")
    } else {
        None
    }
}

/// True for the labels that render the Canopy **launcher** shell — the `main`
/// window today and the future dedicated [`launcher`] window. These are the
/// windows that reduce to the tray when they lose focus (release only, see
/// [`events`]) and that paint their own chrome instead of native decorations.
pub fn is_launcher_label(label: &str) -> bool {
    label == "main" || label == launcher::LAUNCHER_WINDOW_LABEL
}

#[derive(Clone, serde::Serialize)]
struct ProductState<'a> {
    id: &'a str,
    running: bool,
}

/// Tell the launcher a product's running state changed so its Canopy node can
/// flip "In esecuzione" / revert. No-op when the launcher window isn't around.
pub fn emit_product_state(app: &AppHandle, id: &str, running: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.emit("arbor://product-state", ProductState { id, running });
    }
}

/// Product ids that currently have at least one open window. The launcher reads
/// this once on mount to seed its running state (windows opened before its
/// `arbor://product-state` listener existed wouldn't be reflected otherwise).
#[tauri::command]
pub fn list_running_products(app: AppHandle) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for label in app.webview_windows().keys() {
        if let Some(id) = product_id_for_label(label) {
            if !ids.iter().any(|x| x == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// Terminate a product — the launcher's "Stop" action. Uses `destroy()` (not
/// `close()`) so it force-closes every window of the product, bypassing the
/// close-to-tray interception in [`events`]: Stop ALWAYS terminates, so a
/// product can never become an un-killable background window. `Destroyed` then
/// emits `running: false` once the product's last window is gone.
#[tauri::command]
pub fn close_product_window(app: AppHandle, id: String) {
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| product_id_for_label(l) == Some(id.as_str()))
        .cloned()
        .collect();
    tracing::info!("close_product_window({id}): destroying {} window(s): {labels:?}", labels.len());
    for l in labels {
        if let Some(w) = app.get_webview_window(&l) {
            let _ = w.destroy();
        }
    }
}

/// Relaunch the whole app. Used by the fatal "git backend stopped" overlay in
/// the Corvus window: `corvus-be` is spawned once at startup with no live
/// respawn yet, so the only recovery from its death is a full restart. Never
/// returns (the running process is replaced).
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

/// Hop to the main/UI thread before touching WebView2 windows. Window creation
/// off the main thread fails with `HRESULT 0x8007139F` ("resource not in the
/// correct state"); every `open_or_focus` entry point may run on a background
/// thread (global-shortcut handler, async command), so they all route through
/// here. `what` names the window for the error log.
pub fn dispatch_to_main(
    app: &AppHandle,
    what: &'static str,
    f: impl FnOnce(&AppHandle) + Send + 'static,
) {
    let handle = app.clone();
    tracing::info!("dispatch_to_main({what}): posting closure to UI thread");
    if let Err(e) = app.run_on_main_thread(move || {
        tracing::info!("dispatch_to_main({what}): now ON the UI thread — running closure");
        f(&handle);
        tracing::info!("dispatch_to_main({what}): closure returned on UI thread");
    }) {
        tracing::error!("failed to dispatch {what} window to main thread: {e}");
    }
}
