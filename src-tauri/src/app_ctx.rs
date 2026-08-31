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

    // ── Plugin-owned credentials ──────────────────────────────────────────
    //
    // Stored in the same single-keychain-item vault as everything else, under an account
    // `arbor_plugin_types::credentials::account` builds. That function is the only way to
    // name one, and it can only produce `plugin/<name>/<key>` — so the git-provider tokens
    // and issue-tracker keys sitting beside them in the same map are not filtered out of a
    // plugin's reach, they are unnameable from it.
    //
    // Whether the plugin was allowed to ask for this key at all was decided at the API gate,
    // against the slots its manifest declared. This layer re-validates the key's *shape*
    // because that is what keeps the namespace intact, and shape is cheap to check twice.

    // ── Extensions ───────────────────────────────────────────────────────
    //
    // The shell owns the wasm engine, so this is where a call actually happens. `plugin` is
    // carried for the log line and nothing else: the gate ran at the API boundary, and the
    // extension's own capabilities come from ITS manifest, never from the caller's.

    fn ext_surface(&self, _plugin: &str) -> Result<String, String> {
        serde_json::to_string(&crate::ext::surface()).map_err(|e| e.to_string())
    }

    fn ext_call(&self, plugin: &str, spec_json: &str) -> Result<String, String> {
        let spec: crate::ext::CallSpec = serde_json::from_str(spec_json)
            .map_err(|e| format!("arbor.ext.call: {e}"))?;
        tracing::debug!("[{plugin}] ext.call {}@{}/{} {}",
            spec.interface, spec.version, spec.id, spec.method);
        let out = crate::ext::call(&spec)?;
        serde_json::to_string(&out).map_err(|e| e.to_string())
    }

    fn ext_call_to_file(
        &self,
        plugin: &str,
        spec_json: &str,
        file_json: &str,
    ) -> Result<u64, String> {
        let (spec, file) = ext_file_specs("arbor.ext.call_to_file", spec_json, file_json)?;
        tracing::debug!("[{plugin}] ext.call_to_file {}@{}/{} {} -> {}",
            spec.interface, spec.version, spec.id, spec.method, file.path);
        crate::ext::call_to_file(&spec, &file)
    }

    fn ext_call_from_file(
        &self,
        plugin: &str,
        spec_json: &str,
        file_json: &str,
    ) -> Result<String, String> {
        let (spec, file) = ext_file_specs("arbor.ext.call_from_file", spec_json, file_json)?;
        tracing::debug!("[{plugin}] ext.call_from_file {}@{}/{} {} <- {}",
            spec.interface, spec.version, spec.id, spec.method, file.path);
        let out = crate::ext::call_from_file(&spec, &file)?;
        serde_json::to_string(&out).map_err(|e| e.to_string())
    }

    fn oauth_start(&self, plugin: &str, spec_json: &str) -> Result<String, String> {
        let spec: crate::auth::oauth_plugin::StartSpec =
            serde_json::from_str(spec_json).map_err(|e| format!("arbor.oauth.start: {e}"))?;
        // Blocking on the runtime here is bounded and short: `start` binds the loopback
        // listener and builds the URL, then hands the waiting-for-a-human half to a spawned
        // task. It is the wait that would have been unacceptable, and it is not here.
        tauri::async_runtime::block_on(crate::auth::oauth_plugin::start(
            self.handle.clone(),
            plugin.to_string(),
            spec,
        ))
    }

    fn oauth_refresh(&self, plugin: &str, spec_json: &str) -> Result<String, String> {
        let spec: crate::auth::oauth_plugin::RefreshSpec =
            serde_json::from_str(spec_json).map_err(|e| format!("arbor.oauth.refresh: {e}"))?;
        let out = tauri::async_runtime::block_on(crate::auth::oauth_plugin::refresh(
            plugin.to_string(),
            spec,
        ))?;
        serde_json::to_string(&out).map_err(|e| e.to_string())
    }

    fn credential_get(&self, plugin: &str, key: &str) -> Result<Option<String>, String> {
        let account = arbor_plugin_types::prelude::credential_account(plugin, key)
            .map_err(|e| e.to_string())?;
        crate::auth::credential_store::get(&account, "").map_err(|e| e.to_string())
    }

    fn credential_set(&self, plugin: &str, key: &str, value: &str) -> Result<(), String> {
        let account = arbor_plugin_types::prelude::credential_account(plugin, key)
            .map_err(|e| e.to_string())?;
        crate::auth::credential_store::save(&account, "", value).map_err(|e| e.to_string())
    }

    fn credential_delete(&self, plugin: &str, key: &str) -> Result<(), String> {
        let account = arbor_plugin_types::prelude::credential_account(plugin, key)
            .map_err(|e| e.to_string())?;
        crate::auth::credential_store::delete(&account, "").map_err(|e| e.to_string())
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
        // Route through the user's OS-vs-built-in preference
        // (`explorer.reveal_in_builtin`) instead of always hitting the OS
        // opener, so `arbor.ui.open_path` honours "open in Arbor's explorer".
        crate::window::explorer::reveal_path(&self.handle, path)
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

/// Decode the two JSON documents a byte-shaped extension call carries.
///
/// Shared by the two `ext_call_*_file` capabilities above: both take the same pair (where to
/// call, and which file), and both must name themselves in the error — a plugin author reads
/// `arbor.ext.call_to_file: …`, not a decode failure with no origin.
fn ext_file_specs(
    who: &str,
    spec_json: &str,
    file_json: &str,
) -> Result<(crate::ext::CallSpec, crate::ext::FileSpec), String> {
    let spec: crate::ext::CallSpec =
        serde_json::from_str(spec_json).map_err(|e| format!("{who}: {e}"))?;
    let file: crate::ext::FileSpec =
        serde_json::from_str(file_json).map_err(|e| format!("{who}: {e}"))?;
    Ok((spec, file))
}
