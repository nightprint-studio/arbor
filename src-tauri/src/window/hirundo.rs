//! Dedicated **Hirundo** window — the HTTP client of the Bennu suite.
//!
//! Collections of requests live as readable files in the project (`.hirundo/`); `hirundo-be` (spawned lazily
//! by [`crate::ipc::ensure_hirundo_be`]) reads and writes them and sends the requests. The shell owns only the
//! window and the keychain the `@secret` variables live in. Like [`super::garrulus`] it is a single reusable
//! frameless window loading the same `index.html`; the frontend root branches on [`HIRUNDO_WINDOW_LABEL`].

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated Hirundo window.
pub const HIRUNDO_WINDOW_LABEL: &str = "hirundo";

/// Open the dedicated Hirundo window, or focus it if it already exists. WebView2 window creation must run on
/// the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    super::dispatch_to_main(app, "hirundo", create_or_focus);
}

fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(HIRUNDO_WINDOW_LABEL) {
        show_and_focus(&w);
    } else {
        build_hirundo_window(app);
    }
    super::emit_product_state(app, "hirundo", true);
}

fn build_hirundo_window(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, HIRUNDO_WINDOW_LABEL, WebviewUrl::default())
        .title("Hirundo — Arbor")
        .inner_size(1280.0, 820.0)
        .min_inner_size(860.0, 540.0)
        .maximized(true)
        .shadow(true)
        // Built hidden and revealed once the shell has painted (see super::arm_ready_reveal).
        .visible(false)
        .additional_browser_args(WEBVIEW_BROWSER_ARGS);
    match super::native_titlebar(builder).build() {
        Ok(w) => {
            super::apply_product_icon(&w, "hirundo");
            super::arm_ready_reveal(app, HIRUNDO_WINDOW_LABEL)
        }
        Err(e) => tracing::error!("failed to open hirundo window: {e}"),
    }
}

/// IPC entry point so the launcher and the Command Palette can summon the Hirundo window.
///
/// MUST be `async` (see [`super::picus::open_picus_window`]) and must bring the backend up on the BLOCKING
/// pool: `ensure_hirundo_be` parks on synchronous framed-IPC, and a runtime worker blocked there starves the
/// reverse channel the backend asks the keychain over.
#[tauri::command]
pub async fn open_hirundo_window(app: AppHandle) {
    let app_be = app.clone();
    let _ = tokio::task::spawn_blocking(move || crate::ipc::ensure_hirundo_be(&app_be)).await;
    open_or_focus(&app);
}
