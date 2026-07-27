//! The **workspace container** window — several products as tabs in one window.
//!
//! Arbor's default model is one window per product, which is how Windows and
//! Linux users expect it (each window gets its own taskbar button). macOS has no
//! per-window taskbar, so a user with Canopy + Corvus + Bennu open spends the
//! day hunting through ⌘-Tab and Mission Control. This window is the answer: a
//! single container that hosts the **workspace** products (see
//! [`SurfaceKind::Workspace`](super::SurfaceKind)) as tabs, browser-style.
//!
//! What lives in a tab is decided by the frontend (`WorkspaceContainer.svelte`);
//! this module only owns the native window. Two rules shape it:
//!
//!  * **One tab per product.** Every product's frontend state lives in
//!    module-level stores, one set per window — so two Corvus tabs in the same
//!    window would share a single repository state and mirror each other. A
//!    second instance of a product therefore opens its own window (the tab's
//!    "open in new window"), which is also what the multi-monitor case wants.
//!  * **Utility and ambient surfaces stay out.** The File Explorer is freely
//!    multi-instance and Tyto belongs in the tray; neither is something you
//!    alt-tab between all day.
//!
//! Which model is in force is the user's `launcher.window_mode` setting, whose
//! default is per-OS (tabbed on macOS, separate windows elsewhere).

use std::collections::HashSet;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the container. The frontend reads `getCurrentWindow().label`
/// and matches this to mount `WorkspaceContainer`.
pub const WORKSPACE_WINDOW_LABEL: &str = "workspace";

/// Event pushed to an ALREADY-OPEN container asking it to focus (or open) a
/// product tab. A freshly built container can't receive it — its shell isn't
/// listening yet — so the same intent is also parked in [`PENDING_PRODUCT`] and
/// pulled on mount.
const OPEN_PRODUCT_EVENT: &str = "workspace://open-product";

/// Product id the container should show as soon as it mounts. Set by
/// [`open_workspace_window`] and consumed once by [`take_workspace_intent`] —
/// the same pull-flag pattern Tyto uses for its snip intent.
static PENDING_PRODUCT: Mutex<Option<String>> = Mutex::new(None);

/// Open the container (or focus it) and show `product`'s tab.
///
/// `product` is `None` when the user opens the container itself — it then
/// restores whatever it had, or shows the Canopy home tab. WebView2 window
/// creation must run on the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle, product: Option<String>) {
    if let Some(id) = product {
        if let Ok(mut g) = PENDING_PRODUCT.lock() {
            *g = Some(id.clone());
        }
        // An open container gets the intent as an event; a fresh one will pull
        // the flag above on mount. Sending both is harmless — the frontend
        // treats "show this product" as idempotent.
        if let Some(w) = app.get_webview_window(WORKSPACE_WINDOW_LABEL) {
            let _ = w.emit(OPEN_PRODUCT_EVENT, id);
        }
    }
    super::dispatch_to_main(app, "workspace", create_or_focus);
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(WORKSPACE_WINDOW_LABEL) {
        show_and_focus(&w);
    } else {
        build_workspace_window(app);
    }
}

/// Build the container window. Same shape as a product window (maximised,
/// built hidden, revealed on first paint) — it IS where the products live.
fn build_workspace_window(app: &AppHandle) {
    // Logged around `build()` like the Corvus window: creating a webview runs on
    // the UI thread, and if it ever wedges there NOTHING else can open a window
    // again (every `open_*` hops through the same thread). These two lines say
    // whether a failure is in the build or after it.
    tracing::info!("build_workspace_window: calling WebviewWindowBuilder::build() (UI thread)");
    let builder = WebviewWindowBuilder::new(app, WORKSPACE_WINDOW_LABEL, WebviewUrl::default())
        .title("Arbor")
        .inner_size(1320.0, 860.0)
        .min_inner_size(900.0, 600.0)
        // Maximised, like the Corvus window: no `.center()` alongside it (they
        // race and leave the window off-centre at its restore size).
        .maximized(true)
        .shadow(true)
        // Build HIDDEN and reveal once the shell has painted (window_ready) — an
        // opaque WebView2 window would otherwise flash its white default page.
        .visible(false)
        // Must match every other webview's env (see WEBVIEW_BROWSER_ARGS).
        .additional_browser_args(WEBVIEW_BROWSER_ARGS);
    // Native traffic lights on macOS, frameless elsewhere.
    match super::native_titlebar(builder).build() {
        Ok(_) => {
            tracing::info!("build_workspace_window: build() OK");
            super::arm_ready_reveal(app, WORKSPACE_WINDOW_LABEL);
        }
        Err(e) => tracing::error!("failed to open workspace window: {e}"),
    }
}

// ───────────────────────────────────────────────────────────────────────────
//  Hosted products — backends and launcher state
// ───────────────────────────────────────────────────────────────────────────
//
// A product in a tab needs exactly what a product in its own window needs: its
// headless backend running, and the launcher lit up. Neither comes for free
// here — `open_<product>_window` used to own both, and the container bypasses
// it. Without this a Corvus tab loads its shell and every git call answers
// `unknown command: list_workspaces`, because `corvus-be` was never spawned.
//
// The shell also can't infer a tab closing from window events (there is no
// window per tab), so the frontend reports it: [`workspace_tab_opened`] /
// [`workspace_tab_closed`] keep [`HOSTED`] in step, and closing the container
// tears down whatever is left in it — otherwise its backends would outlive the
// window that needed them.

/// Products currently open as tabs of the container.
static HOSTED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn hosted_insert(product: &str) {
    if let Ok(mut g) = HOSTED.lock() {
        g.get_or_insert_with(HashSet::new).insert(product.to_string());
    }
}

fn hosted_remove(product: &str) -> bool {
    HOSTED
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().map(|s| s.remove(product)))
        .unwrap_or(false)
}

/// Drain the hosted set — used when the container itself goes away.
fn hosted_drain() -> Vec<String> {
    HOSTED
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().map(|s| s.drain().collect()))
        .unwrap_or_default()
}

/// Spawn a hosted product's backend, off the runtime workers.
///
/// Mirrors `open_<product>_window`: `ensure_*_be` parks on synchronous
/// framed-IPC and can trigger reverse-channel host round-trips that need FREE
/// runtime workers, so it must run on the blocking pool — doing it inline
/// deadlocks the shell and blanks every window. Idempotent: a no-op when the
/// backend is already attached.
async fn ensure_backend_for(app: &AppHandle, product: &str) {
    let app_be = app.clone();
    let product = product.to_string();
    let _ = tokio::task::spawn_blocking(move || match product.as_str() {
        "corvus" => crate::ipc::ensure_corvus_be(&app_be),
        "bennu" => crate::ipc::ensure_bennu_be(&app_be),
        "merula" => crate::ipc::ensure_merula_be(&app_be),
        "picus" => crate::ipc::ensure_picus_be(&app_be),
        // `home` (Canopy) has no backend of its own.
        _ => {}
    })
    .await;
}

/// A product tab opened: bring its backend up and light the launcher node.
/// Called by the container as it mounts a tab — including tabs restored from
/// the previous session, which never went through [`open_workspace_window`].
#[tauri::command]
pub async fn workspace_tab_opened(app: AppHandle, product: String) {
    if product == "home" {
        return;
    }
    ensure_backend_for(&app, &product).await;
    hosted_insert(&product);
    super::emit_product_state(&app, &product, true);
}

/// A product tab closed: tear its backend down and clear the launcher node —
/// the same contract as closing that product's window.
#[tauri::command]
pub fn workspace_tab_closed(app: AppHandle, product: String) {
    if !hosted_remove(&product) {
        return;
    }
    // Only when the product has no window of its own left, mirroring the
    // window-close path: a detached copy must keep working.
    let still_windowed = app
        .webview_windows()
        .keys()
        .any(|l| super::product_id_for_label(l) == Some(product.as_str()));
    if still_windowed {
        return;
    }
    super::emit_product_state(&app, &product, false);
    crate::ipc::split_broker::detach(&product, "workspace-tab-closed");
}

/// Tear down every product the container was hosting. Called from the window
/// event handler when the container window is destroyed — its tabs die with it,
/// and their backends must not outlive them.
pub fn teardown_hosted(app: &AppHandle) {
    for product in hosted_drain() {
        let still_windowed = app
            .webview_windows()
            .keys()
            .any(|l| super::product_id_for_label(l) == Some(product.as_str()));
        if still_windowed {
            continue;
        }
        super::emit_product_state(app, &product, false);
        crate::ipc::split_broker::detach(&product, "workspace-closed");
    }
}

/// Open/focus the container, optionally on a given product's tab. The launcher
/// calls this instead of `open_<product>_window` when the user's window mode is
/// `tabbed`.
///
/// **Async on purpose — like every other window opener.** Tauri runs sync
/// commands ON the main thread, and this one posts a webview build back to that
/// same thread: the build then re-enters the event loop from inside the IPC
/// handler and WebView2 wedges there. Nothing dramatic is logged — the closure
/// simply never returns — and because every `open_*_window` hops through the UI
/// thread, the whole app stops being able to open ANY window. An async command
/// runs on the runtime instead, so the thread it posts to is free.
#[tauri::command]
pub async fn open_workspace_window(app: AppHandle, product: Option<String>) {
    // Bring the product's backend up BEFORE the tab's shell loads and fires its
    // first BE-required `rpc` — exactly what `open_<product>_window` does, and
    // for the same reason: the shell doesn't retry, it just reports the call as
    // unknown. Skipped when the container is opened without a target product.
    if let Some(id) = product.as_deref() {
        ensure_backend_for(&app, id).await;
    }
    open_or_focus(&app, product);
}

/// Pull the parked "show this product" intent, if any. Called once by the
/// container's shell on mount; returns `None` on every subsequent call.
#[tauri::command]
pub fn take_workspace_intent() -> Option<String> {
    PENDING_PRODUCT.lock().ok().and_then(|mut g| g.take())
}
