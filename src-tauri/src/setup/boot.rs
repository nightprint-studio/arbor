//! Background plugin-boot thread.
//!
//! Plugin loading runs off the UI thread so the webview can mount + render its
//! boot-splash overlay BEFORE the (potentially slow) discover → topo-sort →
//! `load_plugin` pass. The thread emits `arbor://boot-progress` events per
//! plugin and a final `arbor://boot-done` for the splash to dismiss itself.
//!
//! [`spawn`] returns only after the boot thread has acquired the `plugin_host`
//! lock, so every plugin-touching IPC issued by the frontend is guaranteed to
//! queue behind boot (no empty-host race seeding the frontend stores).

use tauri::{Emitter, Manager};

use crate::AppState;

/// Spawn the boot thread and block until it holds the `plugin_host` lock.
pub fn spawn(app: &tauri::App) {
    let handle_for_boot = app.handle().clone();
    // Synchronous handshake: `spawn` returns ONLY after the boot thread has
    // acquired the plugin_host mutex. Without this gate, there's a window
    // between `thread::spawn` returning and the OS actually scheduling the boot
    // thread — during which the WebView can mount and AppShell.onMount can fire
    // IPCs (list_plugin_info, list_plugin_contributions) that win the lock
    // first, find an empty host, and seed frontend stores with empty state.
    let (lock_acquired_tx, lock_acquired_rx) = std::sync::mpsc::sync_channel::<()>(0);
    std::thread::Builder::new()
        .name("arbor-plugin-boot".to_string())
        .spawn(move || boot_thread(handle_for_boot, lock_acquired_tx))
        .expect("failed to spawn arbor-plugin-boot thread");

    // Block here until the boot thread has acquired the plugin_host lock. The
    // send() in the thread is the rendezvous point: after this returns, every
    // plugin-touching IPC issued by the frontend is guaranteed to queue behind
    // boot.
    lock_acquired_rx
        .recv()
        .expect("arbor-plugin-boot thread exited before signalling lock acquisition");
}

fn boot_thread(handle: tauri::AppHandle, lock_acquired_tx: std::sync::mpsc::SyncSender<()>) {
    let state = handle.state::<AppState>();
    let mut host = state
        .plugin_host
        .lock()
        .expect("plugin_host mutex poisoned during boot");
    // Signal `spawn()` that the lock is now held by us. From this point on,
    // every frontend IPC that needs `plugin_host` queues behind us. `send`
    // blocks until `spawn()` calls `recv`, so this is a true rendezvous.
    let _ = lock_acquired_tx.send(());

    wait_for_frontend(&state);

    // PluginHost's app context / api installer / extra roots are wired up in
    // `scheduler::wire` — before this thread acquires the lock — so the boot
    // thread goes straight to `reload()`.

    let plugins_enabled = state
        .config
        .lock()
        .map(|c| c.plugins_enabled)
        .unwrap_or(false);

    // Helper closures: emit the live event AND mirror the payload into shared
    // state so the splash can recover when the WebView mounts after the event
    // has fired.
    let emit_progress = |payload: serde_json::Value| {
        if let Ok(mut slot) = state.boot_progress.lock() {
            *slot = Some(payload.clone());
        }
        let _ = handle.emit("arbor://boot-progress", payload);
    };
    let mark_done = |payload: serde_json::Value| {
        state.boot_done.store(true, std::sync::atomic::Ordering::Release);
        if let Ok(mut slot) = state.boot_progress.lock() {
            *slot = Some(payload.clone());
        }
        let _ = handle.emit("arbor://boot-done", payload);
    };

    if !plugins_enabled {
        tracing::info!("plugin system disabled by config — skipping load");
        mark_done(serde_json::json!({
            "skipped": true,
            "reason":  "plugin system disabled in config",
        }));
        return;
    }

    if let Err(e) = host.reload() {
        tracing::warn!("failed to load plugins during boot: {e}");
        emit_progress(serde_json::json!({
            "phase":   "reload-error",
            "message": format!("Plugin discovery failed: {e}"),
        }));
    }

    emit_progress(serde_json::json!({
        "phase":   "starting-schedulers",
        "message": "Starting plugin schedulers—",
    }));
    host.start_all_schedulers();

    // Match the manual `reload_plugins` command: emit `arbor://plugins-reloaded`
    // so every store/component that refreshes on that signal re-reads with the
    // host fully populated. Without this, listeners attached during AppShell
    // mount sit idle waiting for an event only the manual Refresh would fire.
    let _ = handle.emit("arbor://plugins-reloaded", ());

    mark_done(serde_json::json!({ "skipped": false }));
}

/// Wait for the frontend handshake before emitting any boot events.
/// `BootSplash.onMount` registers the `arbor://boot-progress` /
/// `arbor://boot-done` listeners, then calls the `frontend_ready` IPC which
/// flips this flag. Without the handshake, fast boots can emit and dismiss
/// before listeners exist; with it, events always land. The 5s timeout is a
/// safety net so a wedged / missing frontend can't strand the boot thread.
fn wait_for_frontend(state: &AppState) {
    let (lock, cvar) = &*state.frontend_ready;
    let mut ready = lock
        .lock()
        .expect("frontend_ready mutex poisoned during boot");
    let timeout = std::time::Duration::from_secs(5);
    let deadline = std::time::Instant::now() + timeout;
    while !*ready {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            tracing::warn!(
                "frontend_ready handshake timed out after 5s — proceeding (BootSplash will recover via get_boot_state)"
            );
            break;
        }
        let (g, wait_res) = cvar
            .wait_timeout(ready, remaining)
            .expect("frontend_ready condvar wait poisoned");
        ready = g;
        if wait_res.timed_out() && !*ready {
            tracing::warn!(
                "frontend_ready handshake timed out after 5s — proceeding (BootSplash will recover via get_boot_state)"
            );
            break;
        }
    }
}
