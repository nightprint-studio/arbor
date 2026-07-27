//! Dedicated **Picus** window — the SQL studio product shell.
//!
//! Picus is Arbor's tool for databases and the SQL scripts that build them: a
//! client for Oracle / PostgreSQL, and a maintainer for the per-dialect script
//! repository those databases are installed from. Like [`super::bennu`] it opens
//! in its own frameless window loading the same `index.html`, with the frontend
//! root (`src/routes/+page.svelte`) branching on the window label
//! ([`PICUS_WINDOW_LABEL`]) to mount the Picus shell.
//!
//! Single reusable window, re-summoned rather than duplicated.
//!
//! `picus-be` (the product backend, spawned lazily by
//! [`crate::ipc::ensure_picus_be`]) owns the studio's real work. Today it serves the
//! typed product config and the self-test round-trip; the database and script
//! domains land in the following waves. Until a domain is served, the frontend runs
//! on its fixtures (`src/lib/ipc/picus/mock.ts`) — the same staging Tyto went
//! through before its capture engine landed. The shell here owns only the
//! OS-integration glue (the window) and the credential broker Picus calls back into:
//! **the product keeps no password**.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated Picus window. The frontend reads
/// `getCurrentWindow().label` and matches this to mount the Picus shell.
pub const PICUS_WINDOW_LABEL: &str = "picus";

/// Open the dedicated Picus window, or focus it if it already exists. WebView2
/// window creation must run on the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    super::dispatch_to_main(app, "picus", create_or_focus);
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PICUS_WINDOW_LABEL) {
        show_and_focus(&w);
    } else {
        build_picus_window(app);
    }
    // Light up the launcher's Picus node as running.
    super::emit_product_state(app, "picus", true);
}

/// Build the frameless Picus window.
///
/// `WebviewUrl::default()` resolves to the app's index (`index.html`) — the same
/// entry the launcher/main window uses — so the load path is identical in dev
/// (Vite) and packaged builds. Frameless to match Arbor; the Picus shell paints
/// its own titlebar + window controls. Opens maximised: a five-zone workspace
/// with a data grid wants the screen.
fn build_picus_window(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, PICUS_WINDOW_LABEL, WebviewUrl::default())
        .title("Picus — Arbor")
        .inner_size(1320.0, 860.0)
        .min_inner_size(900.0, 600.0)
        // Opens maximised — a zoom-to-fill, NOT a full-screen Space. No `.center()`:
        // paired with `.maximized(true)` it raced and left the window off-centre at
        // its restore size when maximise didn't take (the macOS frameless bug).
        .maximized(true)
        .shadow(true)
        // Build HIDDEN and reveal once the shell has painted (window_ready) — an
        // opaque WebView2 window would otherwise flash its white default page
        // during load. See super::window_ready / arm_ready_reveal.
        .visible(false)
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS);
    // Native traffic lights on macOS, frameless elsewhere (see super::native_titlebar).
    let res = super::native_titlebar(builder).build();

    match res {
        Ok(_) => super::arm_ready_reveal(app, PICUS_WINDOW_LABEL),
        Err(e) => tracing::error!("failed to open picus window: {e}"),
    }
}

/// IPC entry point so the launcher (and the in-app Command Palette) can summon
/// the Picus window.
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**, and
/// dispatching WebView2 window creation via `run_on_main_thread` from the main
/// thread (while it's blocked inside this command) leaves the new window with an
/// uninitialised webview — a blank window with no devtools. As an async command
/// it runs on the async runtime (a background thread), so the
/// `run_on_main_thread` hop in `open_or_focus` behaves correctly. Same reasoning
/// as [`super::bennu::open_bennu_window`].
#[tauri::command]
pub async fn open_picus_window(app: AppHandle) {
    // Bring up the studio backend before the window's shell loads and fires its
    // first BE-required `rpc` (today `get_picus_config`).
    //
    // CRITICAL: run `ensure_picus_be` on the BLOCKING POOL, never on a runtime
    // worker. It does synchronous framed-IPC (`ChildClient::call` parks on a std
    // `rx.recv()`), and picus-be can fire a reverse-channel host request during
    // startup that the shell answers with `block_on` — which needs FREE runtime
    // workers. Blocking a worker here starves that path → blank-window deadlock that
    // also freezes the launcher. Load-bearing for Picus in particular: the product
    // stores no password, so credential resolution *is* a reverse-channel call.
    // `spawn_blocking` keeps all workers free while we await the backend coming up.
    // Idempotent — a no-op when Picus is re-summoned and the backend is attached.
    let app_be = app.clone();
    let _ = tokio::task::spawn_blocking(move || crate::ipc::ensure_picus_be(&app_be)).await;
    open_or_focus(&app);
}
