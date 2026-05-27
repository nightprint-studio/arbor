//! Shim — re-exports the plugin-core `ApiCtx` and adds the Tauri-shell-only
//! `ApiCtxExt` trait that recovers a real `tauri::AppHandle` from the
//! `Arc<dyn AppCtx>` stored on it.
//!
//! The downcast bridge is intentionally a smell: every ns/* still using
//! `ctx.app_handle()` is a namespace that hasn't yet migrated into a
//! domain crate. As ns/* move out of `src-tauri/src/plugin/api/ns/*`, the
//! `app_handle()` calls disappear and the AppCtx surface grows the
//! corresponding capability instead.

pub use arbor_plugin_core::prelude::ApiCtx;

use crate::app_ctx::TauriAppCtx;

/// Tauri-shell extension on top of plugin-core's `ApiCtx`. Hands back the
/// concrete `tauri::AppHandle` that legacy ns/* installers capture into
/// their closures.
pub trait ApiCtxExt {
    /// Concrete Tauri handle, when the host actually wired one in. `None`
    /// in headless / test runs (where the sandbox is built with no
    /// `AppCtx`) or when the wrapped `AppCtx` is not a `TauriAppCtx`.
    fn app_handle(&self) -> Option<tauri::AppHandle>;
}

impl ApiCtxExt for ApiCtx {
    fn app_handle(&self) -> Option<tauri::AppHandle> {
        let app_ctx = self.app_ctx.as_ref()?;
        let any = app_ctx.as_any();
        let tauri_ctx = any.downcast_ref::<TauriAppCtx>()?;
        Some(tauri_ctx.handle().clone())
    }
}
