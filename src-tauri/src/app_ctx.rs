//! `AppCtx` bridge between Tauri's `AppHandle` and the Tauri-agnostic
//! domain crates (`arbor-scheduler`, future `arbor-plugin-*`, …).
//!
//! Domain crates that need to emit events, locate the Arbor data root, or
//! read the user-focus signal take a `&dyn AppCtx` instead of a Tauri
//! handle. This module implements that trait once on top of `AppHandle`
//! plus the `AppState.app_focused` flag, and the shell crate hands the
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

    fn spawn(&self, fut: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>) {
        tauri::async_runtime::spawn(fut);
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
        // Read the cached active-tab path (kept fresh by `set_active_tab`). The
        // launcher holds no repo registry, and this can run inside a corvus-be
        // reverse call (via a plugin hook), so we must NOT call back into
        // corvus-be here.
        let state = self.handle.state::<crate::AppState>();
        let path = state.active_repo_path.lock().ok()?.clone()?;
        Some(PathBuf::from(path))
    }

    fn open_path(&self, path: &str) -> Result<(), String> {
        use tauri_plugin_opener::OpenerExt;
        self.handle.opener().open_path(path, None::<&str>)
            .map_err(|e| e.to_string())
    }

    fn invoke_host_command(&self, id: &str, ctx_json: &str) {
        // Non-blocking: the plugin host calls this while holding its own lock,
        // so we defer the handler to the async runtime and return immediately.
        // The handler (a regular Tauri command body) may fire plugin hooks that
        // re-lock the host — safe only because the lock is released by then.
        let handle   = self.handle.clone();
        let id       = id.to_string();
        let ctx_json = ctx_json.to_string();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = crate::plugin_host_commands::dispatch(&handle, &id, &ctx_json).await {
                tracing::warn!(target: "plugin", "host command '{id}' failed: {e}");
            }
        });
    }
}
