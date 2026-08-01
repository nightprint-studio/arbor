//! Dedicated **Garrulus** window — the notes product shell.
//!
//! Garrulus is Arbor's knowledge base: a folder of plain `.md` files with YAML
//! frontmatter, `[[wikilinks]]`, `#tags` and callouts — the Obsidian dialect,
//! byte-compatible on purpose — plus the thing that justifies building it, an
//! automatic synchronisation between machines over an abstracted remote (git
//! first). Like [`super::picus`] it opens in its own frameless window loading the
//! same `index.html`, with the frontend root (`src/routes/+page.svelte`) branching
//! on the window label ([`GARRULUS_WINDOW_LABEL`]) to mount the Garrulus shell.
//!
//! Single reusable window, re-summoned rather than duplicated: the vault, the
//! index and the filesystem watcher are per-process state, so a second window
//! would mean a second view of one mutable thing.
//!
//! `garrulus-be` (the product backend, spawned lazily by
//! [`crate::ipc::ensure_garrulus_be`]) owns the vault: discovery, note I/O, note
//! types and templates, the link/search index, the sync remotes and the watcher.
//! The shell here owns only the OS-integration glue (the window) and the
//! credential broker the sync engine calls back into — **the product keeps no
//! token**.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated Garrulus window. The frontend reads
/// `getCurrentWindow().label` and matches this to mount the Garrulus shell.
pub const GARRULUS_WINDOW_LABEL: &str = "garrulus";

/// Open the dedicated Garrulus window, or focus it if it already exists. WebView2
/// window creation must run on the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    super::dispatch_to_main(app, "garrulus", create_or_focus);
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(GARRULUS_WINDOW_LABEL) {
        show_and_focus(&w);
    } else {
        build_garrulus_window(app);
    }
    // Light up the launcher's Garrulus node as running.
    super::emit_product_state(app, "garrulus", true);
}

/// Build the frameless Garrulus window.
///
/// `WebviewUrl::default()` resolves to the app's index (`index.html`) — the same
/// entry the launcher/main window uses — so the load path is identical in dev
/// (Vite) and packaged builds. Frameless to match Arbor; the Garrulus shell paints
/// its own titlebar + window controls. Opens maximised: a note pane flanked by a
/// vault tree and a backlinks panel is a three-column layout, and the editor
/// column is the one that must not end up narrow.
fn build_garrulus_window(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, GARRULUS_WINDOW_LABEL, WebviewUrl::default())
        .title("Garrulus — Arbor")
        .inner_size(1280.0, 840.0)
        .min_inner_size(860.0, 560.0)
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
        Ok(_) => super::arm_ready_reveal(app, GARRULUS_WINDOW_LABEL),
        Err(e) => tracing::error!("failed to open garrulus window: {e}"),
    }
}

/// IPC entry point so the launcher (and the in-app Command Palette) can summon
/// the Garrulus window.
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**, and
/// dispatching WebView2 window creation via `run_on_main_thread` from the main
/// thread (while it's blocked inside this command) leaves the new window with an
/// uninitialised webview — a blank window with no devtools. As an async command
/// it runs on the async runtime (a background thread), so the
/// `run_on_main_thread` hop in `open_or_focus` behaves correctly. Same reasoning
/// as [`super::picus::open_picus_window`].
#[tauri::command]
pub async fn open_garrulus_window(app: AppHandle) {
    // Bring up the notes backend before the window's shell loads and fires its
    // first BE-required `rpc` (the product config + the last vault).
    //
    // CRITICAL: run `ensure_garrulus_be` on the BLOCKING POOL, never on a runtime
    // worker. It does synchronous framed-IPC (`ChildClient::call` parks on a std
    // `rx.recv()`), and garrulus-be can fire a reverse-channel host request during
    // startup that the shell answers with `block_on` — which needs FREE runtime
    // workers. Blocking a worker here starves that path → blank-window deadlock that
    // also freezes the launcher. Load-bearing for Garrulus in particular: the sync
    // engine holds no token, so every git remote it probes resolves its credential
    // over the reverse channel.
    // `spawn_blocking` keeps all workers free while we await the backend coming up.
    // Idempotent — a no-op when Garrulus is re-summoned and the backend is attached.
    let app_be = app.clone();
    let _ = tokio::task::spawn_blocking(move || crate::ipc::ensure_garrulus_be(&app_be)).await;
    open_or_focus(&app);
}
