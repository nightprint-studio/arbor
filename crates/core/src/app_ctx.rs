//! Tauri-agnostic handle into the host process.
//!
//! Domain crates that need to emit events, locate the Arbor data root, or
//! ask whether the user is currently focused on the window take a
//! `&dyn AppCtx` (or `Arc<dyn AppCtx>`) instead of a `tauri::AppHandle`.
//! The Tauri shell crate implements this trait once on top of `AppHandle`;
//! tests implement a lightweight mock.
//!
//! The trait is intentionally minimal — every method is a "the host has
//! this and the domain needs it" capability. New methods are added only
//! when a domain crate actually needs one, never speculatively.
//!
//! No consumer in `arbor-core` itself uses this trait; it lives here so
//! every domain crate can depend on a single common definition without
//! pulling in the Tauri shell.

use std::any::Any;
use std::path::{Path, PathBuf};

pub trait AppCtx: Any + Send + Sync {
    /// Downcast hook so host-specific call sites (e.g. the Tauri shell's
    /// per-namespace installers that still need a real `tauri::AppHandle`)
    /// can recover the concrete impl from a `&dyn AppCtx`. Domain crates
    /// should never call this — the existence of a downcast is a smell that
    /// a capability is missing from the trait surface.
    fn as_any(&self) -> &dyn Any;

    /// Emit a frontend event with a JSON payload. Equivalent to
    /// `tauri::AppHandle::emit(event, payload)` on the Tauri impl.
    fn emit(&self, event: &str, payload: serde_json::Value);

    /// Root of Arbor's on-disk state (typically the value of
    /// [`crate::paths::arbor_config_dir`]). Exposed through the trait so
    /// hosts that rebase Arbor under a portable directory can override it
    /// without monkey-patching the global helper.
    fn arbor_dir(&self) -> &Path;

    /// Whether the Arbor window currently has user focus. Used by
    /// throughput-sensitive background loops (auto-refresh, polling) to
    /// back off while the user is in another app.
    fn is_focused(&self) -> bool;

    /// Append a line to the Plugin Logs panel (the in-memory ring buffer
    /// that streams to the frontend via `arbor://plugin-log` events).
    ///
    /// `level` is one of `"debug" | "info" | "warn" | "error"`. `plugin`
    /// is the offending plugin's name. `message` is the human-readable
    /// payload. Default is a no-op so headless hosts (CLI, tests) don't
    /// need to wire up a buffer.
    fn record_plugin_log(&self, _level: &str, _plugin: &str, _message: &str) {}

    /// Path of the repository currently visible in the active tab, if any.
    /// Used by host-pure namespaces (`arbor.settings.read_project`, …) that
    /// need to scope per-repo state without depending on a shell-side
    /// `AppState`. Default is `None` so headless / test hosts trivially
    /// satisfy the contract.
    fn active_repo_path(&self) -> Option<PathBuf> { None }

    /// Hand a file/folder path to the OS' default handler (Explorer / Finder
    /// / xdg-open). Backs `arbor.ui.open_path`. Default errors out so headless
    /// hosts surface a clear "unsupported" rather than silently succeeding.
    fn open_path(&self, _path: &str) -> Result<(), String> {
        Err("open_path: not supported by this host".to_string())
    }
}
