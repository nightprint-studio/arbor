//! Lua-facing branding emitters — the `arbor://*` broadcasts used by the
//! `arbor.ui.set_branding` / `arbor.ui.clear_branding` Lua namespace.
//!
//! The IPC readers/notifiers (`get_branding`, `notify_theme_changed`) have
//! moved to the platform backend (`ipc/platform/branding.rs`). What stays here
//! is the egress that takes an `AppHandle` and emits `arbor://*` events, which
//! the in-process Lua API calls directly.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::branding::Branding;

#[derive(Serialize, Clone)]
pub struct BrandingDto {
    pub logo_svg:         Option<String>,
    pub window_icon_path: Option<String>,
    pub owner:            Option<String>,
}

impl From<Branding> for BrandingDto {
    fn from(b: Branding) -> Self {
        Self {
            logo_svg:         b.logo_svg,
            window_icon_path: b.window_icon_path,
            owner:            b.owner,
        }
    }
}

/// Helper for the Lua API to broadcast a branding change.  Lives here so
/// the `set_branding` / `clear_branding` Lua closures don't have to
/// re-emit the same event payload twice.
pub fn emit_branding_changed(app: &AppHandle, current: &Branding) {
    let _ = app.emit("arbor://branding-changed", BrandingDto::from(current.clone()));
}

/// Helper for the Lua API to broadcast a theme-token overlay change.
/// `vars` may be empty to signal "clear my overlay".
pub fn emit_theme_overlay(app: &AppHandle, plugin: &str, vars: &serde_json::Value) {
    let _ = app.emit("arbor://theme-overlay", serde_json::json!({
        "plugin": plugin,
        "vars":   vars,
    }));
}
