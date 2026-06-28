//! `arbor://` deep-link registration + routing (desktop, single-instance builds).
//!
//! Registers the URI scheme at runtime (so `--no-bundle` dev builds work) and
//! routes received URLs through `DeepLinkBuffer` — emitting to the frontend
//! immediately on the warm path, or buffering until `deep_link_ready` on cold
//! start (webview not yet booted).

#[cfg(all(desktop, any(not(debug_assertions), feature = "deep-link-dev")))]
pub fn register(app: &tauri::App) {
    use tauri::Manager;
    use tauri_plugin_deep_link::DeepLinkExt;

    if let Err(e) = app.deep_link().register("arbor") {
        tracing::warn!("failed to register arbor:// scheme: {e}");
    }
    let handle_dl = app.handle().clone();
    let buffer = app.state::<crate::AppState>().deep_link_buffer.clone();

    // Runtime opens (warm path + URLs forwarded by the single-instance plugin's
    // `deep-link` feature).
    let buffer_for_runtime = buffer.clone();
    let handle_for_runtime = handle_dl.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            tracing::info!("deep-link received: {url}");
            buffer_for_runtime.push_or_emit(&handle_for_runtime, url.to_string());
        }
        // Deep links are Git actions (commit/branch/MR jumps) → they belong to
        // the Corvus window, not the launcher ("main"). Open/focus it and make
        // sure corvus-be is up (the buffer above already emitted/queued the URL;
        // once the Corvus AppShell mounts it calls `deep_link_ready`, flushing
        // the buffer to its listener). Do it off this callback thread, matching
        // `open_corvus_window`'s safe context (the `Hello` read blocks).
        let h = handle_for_runtime.clone();
        tauri::async_runtime::spawn(async move {
            crate::ipc::ensure_corvus_be(&h);
            crate::window::corvus::open_or_focus(&h);
        });
    });

    // Cold-start URLs — when the OS launched Arbor by clicking a link, the URL
    // is sitting in argv but `on_open_url` may not re-fire for it depending on
    // the platform. Drain `get_current()` defensively into the same buffer.
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            tracing::info!("deep-link cold-start: {url}");
            buffer.push_or_emit(&handle_dl, url.to_string());
        }
    }
}
