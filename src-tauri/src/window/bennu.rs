//! Dedicated **Bennu** window — the Java-editor / analysis product shell.
//!
//! Bennu is Arbor's Java editor + domain-analysis engine for legacy enterprise
//! stacks (Struts2 / JSP / Entando-jAPS / Spring-XML / JDBC-DAO). Like [`super::corvus`]
//! it opens in its own frameless window loading the same `index.html`, with the
//! frontend root (`src/routes/+page.svelte`) branching on the window label
//! ([`BENNU_WINDOW_LABEL`]) to mount the Bennu editor shell.
//!
//! Single reusable window, re-summoned rather than duplicated. The analysis backend
//! (`bennu-be`) is spawned **lazily** the first time this window opens (see
//! [`open_bennu_window`]) and then shared process-wide — the launcher and the other
//! product windows never start it (they never touch Java analysis).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated Bennu window. The frontend reads
/// `getCurrentWindow().label` and matches this to mount the Bennu editor shell.
pub const BENNU_WINDOW_LABEL: &str = "bennu";

/// Open the dedicated Bennu window, or focus it if it already exists. WebView2
/// window creation must run on the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    super::dispatch_to_main(app, "bennu", create_or_focus);
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(BENNU_WINDOW_LABEL) {
        show_and_focus(&w);
    } else {
        build_bennu_window(app);
    }
    // Light up the launcher's Bennu node as "In esecuzione".
    super::emit_product_state(app, "bennu", true);
}

/// Build the frameless Bennu window.
///
/// `WebviewUrl::default()` resolves to the app's index (`index.html`) — the same
/// entry the launcher/main window uses — so the load path is identical in dev (Vite)
/// and packaged builds. Frameless to match Arbor; the Bennu shell paints its own
/// titlebar + window controls. Opens maximised: a full IDE-style editor wants the
/// screen.
fn build_bennu_window(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, BENNU_WINDOW_LABEL, WebviewUrl::default())
        .title("Bennu — Arbor")
        .inner_size(1320.0, 860.0)
        .min_inner_size(900.0, 600.0)
        // Opens maximised — a zoom-to-fill, NOT a full-screen Space. No `.center()`:
        // paired with `.maximized(true)` it raced and left the window off-centre at
        // its restore size when maximise didn't take (the macOS frameless bug).
        .maximized(true)
        .shadow(true)
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
        Ok(w) => {
            super::apply_product_icon(&w, "bennu");
            super::arm_ready_reveal(app, BENNU_WINDOW_LABEL)
        }
        Err(e) => tracing::error!("failed to open bennu window: {e}"),
    }
}

/// IPC entry point so the launcher (and the in-app Command Palette) can summon the
/// Bennu window.
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**, and
/// dispatching WebView2 window creation via `run_on_main_thread` from the main thread
/// (while it's blocked inside this command) leaves the new window with an
/// uninitialised webview — a blank window with no devtools. As an async command it
/// runs on the async runtime (a background thread), so the `run_on_main_thread` hop
/// in `open_or_focus` behaves correctly. Same reasoning as
/// [`super::corvus::open_corvus_window`].
#[tauri::command]
pub async fn open_bennu_window(app: AppHandle) {
    // Bring up the analysis backend before the window's shell loads and fires its
    // first BE-required `rpc` (e.g. `bennu_capabilities`).
    //
    // CRITICAL: run `ensure_bennu_be` on the BLOCKING POOL, never on a runtime
    // worker. It does synchronous framed-IPC (`ChildClient::call` parks on a std
    // `rx.recv()`), and bennu-be can fire a reverse-channel host request during
    // startup that the shell answers with `block_on` — which needs FREE runtime
    // workers. Blocking a worker here starves that path → blank-window deadlock that
    // also freezes the launcher. `spawn_blocking` keeps all workers free while we
    // await the backend coming up. Idempotent — a no-op when Bennu is re-summoned and
    // the backend is already attached. Same shape as `corvus::open_corvus_window`.
    let app_be = app.clone();
    let _ = tokio::task::spawn_blocking(move || crate::ipc::ensure_bennu_be(&app_be)).await;
    open_or_focus(&app);
}
