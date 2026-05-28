//! `AppCtx` bridge between Tauri's `AppHandle` and the Tauri-agnostic
//! domain crates (`arbor-scheduler`, future `arbor-plugin-*`, …).
//!
//! Domain crates that need to emit events, locate the Arbor data root, or
//! read the user-focus signal take a `&dyn AppCtx` instead of a Tauri
//! handle. This module implements that trait once on top of `AppHandle`
//! + the `AppState.app_focused` flag, and the shell crate hands the
//! resulting trait object to every consumer (scheduler today; more crates
//! as the refactor progresses).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arbor_core::prelude::AppCtx;
use tauri::{AppHandle, Emitter, Manager};

pub struct TauriAppCtx {
    handle:    AppHandle,
    focused:   Arc<AtomicBool>,
    arbor_dir: PathBuf,
}

impl TauriAppCtx {
    pub fn new(handle: AppHandle, focused: Arc<AtomicBool>) -> Self {
        Self {
            handle,
            focused,
            arbor_dir: arbor_core::prelude::arbor_config_dir(),
        }
    }

    /// Convenience constructor for call sites that only need `emit` /
    /// `record_plugin_log` and don't have access to the global focus flag.
    /// `is_focused` reports `false` — safe default because every loop that
    /// throttles on focus also has access to the real `AppState.app_focused`.
    pub fn from_handle(handle: AppHandle) -> Self {
        Self::new(handle, Arc::new(AtomicBool::new(false)))
    }
}

impl TauriAppCtx {
    /// Internal accessor for the wrapped `AppHandle`. Used by the
    /// src-tauri-side `ApiCtxExt::app_handle()` shim that bridges plugin-core's
    /// `Arc<dyn AppCtx>` back to a concrete `tauri::AppHandle` for the ns/*
    /// installers that haven't yet migrated to their own domain crates.
    pub fn handle(&self) -> &AppHandle {
        &self.handle
    }
}

impl AppCtx for TauriAppCtx {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Err(e) = self.handle.emit(event, payload) {
            tracing::warn!("AppCtx emit '{event}' failed: {e}");
        }
    }

    fn arbor_dir(&self) -> &Path {
        &self.arbor_dir
    }

    fn is_focused(&self) -> bool {
        self.focused.load(Ordering::Relaxed)
    }

    fn record_plugin_log(&self, level: &str, plugin: &str, message: &str) {
        crate::plugin_logs::record(&self.handle, level, plugin, message.to_string());
    }

    fn active_repo_path(&self) -> Option<PathBuf> {
        let state = self.handle.state::<crate::AppState>();
        let tab_id = state.active_tab_id.lock().ok()?.clone()?;
        let mut repos = state.repos.lock().ok()?;
        repos.get(&tab_id).ok().map(|r| PathBuf::from(&r.path))
    }

    fn open_path(&self, path: &str) -> Result<(), String> {
        use tauri_plugin_opener::OpenerExt;
        self.handle.opener().open_path(path, None::<&str>)
            .map_err(|e| e.to_string())
    }
}
