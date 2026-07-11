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
//! - [`tyto`] — the screen-recorder control panel (`tyto`), plus its OS-global
//!   shortcut.
//! - [`corvus`] — the Git product window (`corvus`). Today the Git UI also
//!   loads in `main`; this is the seed of the launcher split, where `main`
//!   becomes the launcher and Corvus opens as a product window.
//! - [`bennu`] — the Java-editor / analysis product window (`bennu`). Spawns its
//!   own `bennu-be` backend lazily, exactly like Corvus.
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

pub mod bennu;
pub mod corvus;
pub mod events;
pub mod explorer;
pub mod hud;
pub mod launcher;
pub mod merula;
pub mod placement;
pub mod tyto;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow, WebviewWindowBuilder};

/// WebView2 additional browser args, shared by **every** Arbor window. Every
/// WebView2 instance in the process shares one user-data-folder + environment,
/// so creating a second webview with *different* env options fails with
/// `HRESULT 0x8007139F` (ERROR_INVALID_STATE). Must also match the `main`
/// window's `additionalBrowserArgs` in `tauri.conf.json`.
pub const WEBVIEW_BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,InterestFeedContentSuggestions,WebRTC,AutofillServerCommunication";

// ───────────────────────────────────────────────────────────────────────────
//  Window chrome — native traffic lights on macOS, frameless elsewhere
// ───────────────────────────────────────────────────────────────────────────
//
// Every Arbor product/launcher window paints its own title bar. On Windows and
// Linux it also paints its own min/max/close (IntelliJ-style, flush right) and
// the OS window stays frameless (`decorations(false)`).
//
// On macOS that model fights the platform: users expect the real traffic lights
// (top-left), the native hover menu (window tiling — "Move & Resize"), and the
// green button doing a *zoom* rather than opening a full-screen Space. Faux CSS
// buttons can provide none of it. So on macOS we keep native decorations but
// hide the OS title bar (`TitleBarStyle::Overlay` + `hidden_title`): the real
// traffic lights float over our custom title bar, nudged down to sit centered in
// it. The frontend hides its faux controls and reserves a left gutter on macOS.

/// Horizontal inset (logical px) of the macOS traffic-light cluster from the
/// window's left edge.
#[cfg(target_os = "macos")]
const MAC_TRAFFIC_LIGHT_X: f64 = 19.0;
/// Vertical inset (logical px). tao grows the native title-bar container to
/// `button_height + y` and pins it to the window top, so a LARGER `y` pushes the
/// lights DOWN. At the small default the cluster centers in a ~28px band and
/// reads high against our 42px bar (`--titlebar-h`); ~22 drops it to the bar's
/// centre. The position is fixed at build time, so a compact bar sits slightly
/// low — an accepted minor offset.
#[cfg(target_os = "macos")]
const MAC_TRAFFIC_LIGHT_Y: f64 = 22.0;

/// Apply Arbor's window chrome to a builder. macOS → native Overlay title bar
/// (real traffic lights over the custom bar); elsewhere → frameless. Call it in
/// place of `.decorations(false)` on every window that renders an Arbor title
/// bar. NOT for chromeless overlays (the recording HUD, the drag ghost) — those
/// stay frameless directly.
pub fn native_titlebar<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(target_os = "macos")]
    {
        use tauri::{LogicalPosition, TitleBarStyle};
        builder
            .decorations(true)
            .title_bar_style(TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(LogicalPosition::new(
                MAC_TRAFFIC_LIGHT_X,
                MAC_TRAFFIC_LIGHT_Y,
            ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder.decorations(false)
    }
}

/// Bring a window to the foreground: undo a minimize, show it, take focus. The
/// idempotent three-step every "focus the existing window" path repeats.
pub fn show_and_focus(w: &WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

// ───────────────────────────────────────────────────────────────────────────
//  Anti-white-flash window reveal — the app-wide best practice
// ───────────────────────────────────────────────────────────────────────────
//
// Every Arbor window is an OPAQUE WebView2 (a transparent one gets no input on
// Windows — the documented trap), so during load it paints its white default page
// for a beat before the Svelte shell mounts: a window built *visible* shows a white
// flash. The fix, used by EVERY launcher/product window: build with `.visible(false)`
// and reveal it only once its frontend has painted.
//
// Two centralized pieces make this one pattern instead of per-window copies:
//  • [`window_ready`] — a GENERIC command each shell calls once painted. The window
//    reveals ITSELF through the injected handle, so a single command serves them all
//    (no `foo_ready` per window).
//  • [`arm_ready_reveal`] — the safety net: reveal the window anyway after a short
//    delay, so a frontend that never signals (crash, disabled JS) can't leave the
//    window stuck hidden.
//
// The shell fires the signal from `src/routes/+page.svelte` after the shell mounts +
// two frames. Overlays with deliberate visibility/focus semantics opt out there (the
// drag ghost is shown per-drag, not on a persistent reveal).

/// Delay before the ready-fallback reveals a built-hidden window even if its frontend
/// never signalled — long enough to let a healthy shell paint first.
const READY_FALLBACK_MS: u64 = 800;

/// Arm the safety-net reveal for a window built with `.visible(false)`: after
/// [`READY_FALLBACK_MS`] show it regardless, so a frontend that never calls
/// [`window_ready`] can't leave the window stuck hidden. `show` is idempotent, so
/// racing the real ready signal is harmless. Call once, right after a successful build.
pub fn arm_ready_reveal(app: &AppHandle, label: &str) {
    let app = app.clone();
    let label = label.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(READY_FALLBACK_MS));
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.show();
        }
    });
}

/// Reveal the calling window once its frontend has painted — the app-wide
/// anti-white-flash reveal. Generic across every window built hidden: the window
/// reveals ITSELF through the injected handle, so one command serves them all.
/// Idempotent (`show`/`set_focus` on an already-visible window are effectively no-ops
/// for our windows). The `window` arg is the caller's own window, injected by Tauri.
#[tauri::command]
pub fn window_ready(window: WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
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
    } else if label == tyto::TYTO_WINDOW_LABEL {
        // Tyto is a single-window product (no `tyto-N`).
        Some("tyto")
    } else if label == bennu::BENNU_WINDOW_LABEL || label.starts_with("bennu-") {
        Some("bennu")
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
