//! `branding` domain — in-memory branding overrides + the theme-change
//! notifier, routed through the platform backend.
//!
//! Branding is owned by `AppState.branding`; this file only exposes the thin
//! snapshot reader (`get_branding`) and the theme-change notifier
//! (`notify_theme_changed`). Lua plugins write through the
//! `arbor.ui.set_branding` / `arbor.ui.clear_branding` namespace, whose
//! `arbor://*` emitters stay inline in the command module (they take an
//! `AppHandle`).
//!
//! `notify_theme_changed` fired the `on_theme_changed` plugin hook inline as a
//! Tauri command. Migrated here it returns the change descriptor only; the
//! fire-and-forget hook now belongs in the shell's generic `rpc` post-hooks
//! path so it runs exactly once regardless of in-process vs out-of-process
//! dispatch. See the `postHooksArms` entry reported by the migration.

use std::collections::HashMap;

use serde::Serialize;

use crate::branding::Branding;
use crate::error::AppError;
use crate::ipc::platform;
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
#[platform::handler(program = "platform")]
fn get_branding(state: &AppState) -> Result<BrandingDto, AppError> {
    Ok(state.branding.snapshot().into())
}

/// Tell the backend that the active theme just changed (or that a plugin
/// applied / removed an in-memory token overlay).
///
/// The `on_theme_changed` fan-out to every plugin's handler is fired by the
/// shell's `rpc` post-hooks path (see `postHooksArms`), not inline here, so it
/// runs exactly once whether the method is served in-process or out-of-process.
///
/// `vars` is the *effective* set of CSS variables in force after the change
/// (active theme + any plugin overlays merged).  `source` is one of
/// `"user" | "plugin" | "init"` — purely informational, plugins use it to
/// avoid re-reacting to their own writes.
#[platform::handler(program = "platform")]
fn notify_theme_changed(
    _state:     &AppState,
    _theme_id:   String,
    _theme_name: String,
    _vars:       HashMap<String, String>,
    _source:     String,
) -> Result<(), AppError> {
    Ok(())
}
