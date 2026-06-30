//! Dedicated **Corvus** window — the Git product shell.
//!
//! Corvus is Arbor's Git GUI (branches, commits, diff, MR/PR, …). Today it also
//! loads in the `main` window, but the app is moving toward a launcher model
//! (JetBrains-Toolbox-like): `main` becomes the launcher, and each product —
//! Corvus, merula, … — opens in its own window. This module is the seed of that
//! split: a frameless window that loads the same `index.html`, with the
//! frontend root (`src/routes/+page.svelte`) branching on the window label
//! ([`CORVUS_WINDOW_LABEL`]) to mount the Git `AppShell`.
//!
//! Single reusable window, re-summoned rather than duplicated — like
//! [`super::merula`]. The Git backend (`corvus-be`) is spawned **lazily** the
//! first time this window opens (see [`open_corvus_window`]) and then shared
//! process-wide across Corvus tabs — the launcher and the non-git product
//! windows never start it.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated Corvus window. The frontend reads
/// `getCurrentWindow().label` and matches this to mount the Git `AppShell`.
pub const CORVUS_WINDOW_LABEL: &str = "corvus";

/// Open the dedicated Corvus window, or focus it if it already exists. WebView2
/// window creation must run on the main/UI thread — see
/// [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    super::dispatch_to_main(app, "corvus", create_or_focus);
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(CORVUS_WINDOW_LABEL) {
        tracing::info!("corvus create_or_focus: existing window found — show_and_focus");
        show_and_focus(&w);
        tracing::info!("corvus create_or_focus: show_and_focus returned");
    } else {
        tracing::info!("corvus create_or_focus: no window — building a fresh one");
        build_corvus_window(app);
        tracing::info!("corvus create_or_focus: build returned");
    }
    // Light up the launcher's Corvus node as "In esecuzione".
    super::emit_product_state(app, "corvus", true);
}

/// Build the frameless Corvus window.
///
/// `WebviewUrl::default()` resolves to the app's index (`index.html`) — the same
/// entry the launcher/main window uses — so the load path is identical in dev
/// (Vite) and packaged builds. Frameless to match Arbor; the Git `AppShell`
/// paints its own titlebar + window controls. Opens maximised: a full IDE-style
/// Git workspace wants the screen.
fn build_corvus_window(app: &AppHandle) {
    tracing::info!("build_corvus_window: calling WebviewWindowBuilder::build() (UI thread)");
    let res = WebviewWindowBuilder::new(app, CORVUS_WINDOW_LABEL, WebviewUrl::default())
        .title("Corvus — Arbor")
        .inner_size(1320.0, 860.0)
        .min_inner_size(900.0, 600.0)
        .maximized(true)
        .decorations(false)
        .shadow(true)
        .center()
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS)
        .build();

    match res {
        Ok(_) => tracing::info!("build_corvus_window: build() OK"),
        Err(e) => tracing::error!("failed to open corvus window: {e}"),
    }
}

/// IPC entry point so the launcher (and the in-app Command Palette) can summon
/// the Corvus window.
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**, and
/// dispatching WebView2 window creation via `run_on_main_thread` from the main
/// thread (while it's blocked inside this command) leaves the new window with an
/// uninitialised webview — a blank window with no devtools. As an async command
/// it runs on the async runtime (a background thread), so the
/// `run_on_main_thread` hop in `open_or_focus` behaves correctly. Same reasoning
/// as [`super::merula::open_merula_window`].
#[tauri::command]
pub async fn open_corvus_window(app: AppHandle) {
    // Bring up the Git backend before the window's AppShell loads and fires its
    // first BE-required `rpc` (e.g. `list_workspaces`).
    //
    // CRITICAL: run `ensure_corvus_be` on the BLOCKING POOL, never on a runtime
    // worker. It does synchronous framed-IPC (`ChildClient::call` parks on a
    // std `rx.recv()`), and corvus-be can fire a reverse-channel credential
    // request during startup (`__session`/`__maybe_refresh`/`__git_credentials`)
    // that the shell answers with `block_on` over OAuth/keyring — which needs
    // FREE runtime workers. Blocking a worker here (the old inline call did)
    // starves that path: the credential round-trip can't progress, corvus-be
    // never replies, `rx.recv()` never returns, and because the launcher's own
    // IPC shares those workers, every window goes blank — a deadlock that also
    // freezes the launcher. `spawn_blocking` keeps all workers free while we
    // await the backend coming up. Idempotent — a no-op when Corvus is
    // re-summoned and the backend is already attached.
    tracing::info!("open_corvus_window: ensuring corvus-be (on blocking pool)");
    let app_be = app.clone();
    let _ = tokio::task::spawn_blocking(move || crate::ipc::ensure_corvus_be(&app_be)).await;
    tracing::info!("open_corvus_window: ensure returned — creating/focusing window");
    open_or_focus(&app);
}
