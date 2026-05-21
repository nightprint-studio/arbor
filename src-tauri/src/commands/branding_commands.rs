//! IPC bridge for the in-memory branding overrides + the on_theme_changed
//! hook trigger. Branding is owned by `AppState.branding`; this file only
//! exposes thin readers/notifiers — Lua plugins write through the
//! `arbor.ui.set_branding` / `arbor.ui.clear_branding` namespace.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::branding::Branding;
use crate::error::AppError;
use crate::AppState;

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

/// Snapshot of the current branding state — frontend reads this on init so
/// the title-bar / welcome-screen logo is correct on first paint, then
/// keeps in sync via the `arbor://branding-changed` event.
#[tauri::command]
pub fn get_branding(state: State<'_, AppState>) -> Result<BrandingDto, AppError> {
    Ok(state.branding.snapshot().into())
}

/// Tell the backend that the active theme just changed (or that a plugin
/// applied / removed an in-memory token overlay).  Fans out to every
/// plugin's `on_theme_changed` handler.
///
/// `vars` is the *effective* set of CSS variables in force after the change
/// (active theme + any plugin overlays merged).  `source` is one of
/// `"user" | "plugin" | "init"` — purely informational, plugins use it to
/// avoid re-reacting to their own writes.
#[tauri::command]
pub fn notify_theme_changed(
    state:      State<'_, AppState>,
    theme_id:   String,
    theme_name: String,
    vars:       std::collections::HashMap<String, String>,
    source:     String,
) -> Result<(), AppError> {
    let host = state.lock_plugin_host()?;
    let ctx = serde_json::json!({
        "theme_id":   theme_id,
        "theme_name": theme_name,
        "vars":       vars,
        "source":     source,
    });
    let _ = host.fire_hook("on_theme_changed", &ctx.to_string());
    Ok(())
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
