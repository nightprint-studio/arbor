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

use std::path::Path;

pub trait AppCtx: Send + Sync {
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
}
