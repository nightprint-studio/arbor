//! Boot / focus / active-tab handshake — the keep-shell remainder of the plugin
//! command surface.
//!
//! Everything that mutates the plugin runtime, fires hooks, or reloads the host
//! migrated to the generic router ([`crate::ipc::platform::plugin`]). What stays
//! here are the four commands that return **no `Result`** (so the handler macro
//! can't wrap them): the boot-splash handshake (`get_boot_state` /
//! `frontend_ready`) and the focus / active-tab notifications (`set_app_focus` /
//! `set_active_tab`). They're called very early / very often and are deliberately
//! kept as thin Tauri commands.

use std::sync::atomic::Ordering;

use tauri::State;

use arbor_plugin_types::prelude::hook_names;

use crate::AppState;

// ---------------------------------------------------------------------------
// Boot handshake — splash readiness.
// ---------------------------------------------------------------------------

/// Snapshot of the boot loader's current state. The splash component polls
/// this on mount as a safety net for dev-mode HMR remounts where the listener
/// attaches after `arbor://boot-done` has already fired (the `frontend_ready`
/// handshake covers first-launch; this covers re-mount).
#[tauri::command]
pub fn get_boot_state(state: State<'_, AppState>) -> serde_json::Value {
    let done = state.boot_done.load(Ordering::Acquire);
    let progress = state.boot_progress.lock().ok().and_then(|g| g.clone());
    serde_json::json!({
        "done":     done,
        "progress": progress,
    })
}

/// Frontend handshake — `BootSplash.onMount` calls this once both the
/// `arbor://boot-progress` and `arbor://boot-done` listeners are registered.
/// The boot thread parks on `state.frontend_ready` until this flips, then
/// emits its events. Idempotent: subsequent calls are a no-op.
#[tauri::command]
pub fn frontend_ready(state: State<'_, AppState>) {
    let (lock, cvar) = &*state.frontend_ready;
    if let Ok(mut ready) = lock.lock() {
        if !*ready {
            *ready = true;
            cvar.notify_all();
        }
    }
}

// ---------------------------------------------------------------------------
// App focus / active-tab state — called by the frontend on visibility changes.
// ---------------------------------------------------------------------------

/// Two things happen when the focus state changes:
///  1. `app_focused` is updated so focus-gated plugin schedulers can skip
///     firing while the window is in the background.
///  2. On Windows, EcoQoS / Efficiency Mode is toggled (by the native
///     WindowEvent::Focused handler in lib.rs) so Task Manager shows the green
///     leaf icon while Arbor is not in the foreground.
#[tauri::command]
pub fn set_app_focus(state: State<'_, AppState>, focused: bool) {
    let t0 = std::time::Instant::now();
    let prev = state.app_focused.swap(focused, Ordering::Relaxed);
    tracing::info!(
        target: "arbor::focus",
        "set_app_focus(focused={focused}) prev={prev} took={}µs",
        t0.elapsed().as_micros()
    );
}

/// Inform the backend which tab is currently active in the frontend.
/// Used by `arbor.repo.fetch_active_tab()` to know which repo to operate on.
/// Also fires the `arbor:tab_switch` plugin hook when a real tab is activated.
#[tauri::command]
pub fn set_active_tab(state: State<'_, AppState>, tab_id: Option<String>) {
    if let Ok(mut id) = state.active_tab_id.lock() {
        *id = tab_id.clone();
    }
    if let Some(ref tid) = tab_id {
        // Look up the repo path (so plugins can use arbor.settings.project) by
        // asking corvus-be — the launcher keeps no repo registry. The name is the
        // workdir basename, identical to the old `RepoInfo.name`. This is a forward
        // call (FE → shell), so resolving via corvus-be here is safe.
        let repo_info: Option<(String, String)> =
            crate::ipc::resolve_tab_path(state.inner(), tid).ok().map(|path| {
                let name = std::path::Path::new(&path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (path, name)
            });
        // Cache the active repo path so the plugin host + reverse-channel pipeline
        // cwd fallback can read it without a re-entrant call into corvus-be.
        if let Ok(mut p) = state.active_repo_path.lock() {
            *p = repo_info.as_ref().map(|(path, _)| path.clone());
        }
        state.fire_hook(hook_names::arbor::TAB_SWITCH, serde_json::json!({
            "tab_id": tid,
            "path":   repo_info.as_ref().map(|(p, _)| p.as_str()).unwrap_or(""),
            "name":   repo_info.as_ref().map(|(_, n)| n.as_str()).unwrap_or(""),
        }));
    } else if let Ok(mut p) = state.active_repo_path.lock() {
        // No active tab → clear the cached path.
        *p = None;
    }
}
