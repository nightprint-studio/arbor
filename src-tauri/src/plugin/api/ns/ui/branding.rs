//! `arbor.ui.set_branding` / `arbor.ui.clear_branding` /
//! `arbor.ui.set_theme_tokens` / `arbor.ui.clear_theme_tokens`.
//!
//! All four are RAM-only: nothing is persisted across sessions. A reload of
//! Arbor restores the bundled identity unless the same plugin re-applies
//! the override during its `on_plugin_load` handler.
//!
//! Branding (logo) lives in `AppState.branding` so backend-rendered
//! artefacts (HTML stats export) can pick it up too. Theme token overlays
//! live entirely on the frontend — the Rust side just rebroadcasts them
//! via `arbor://theme-overlay` so multiple webviews / panels stay in sync.

use mlua::{Lua, Table};
use tauri::{AppHandle, Manager};
use tauri::image::Image;

use crate::commands::branding_commands::{emit_branding_changed, emit_theme_overlay};
use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

/// Push `path` to the OS window-icon API so the taskbar / Alt-Tab list /
/// window chrome reflect the override. Tauri requires a rasterised buffer
/// (PNG / ICO); SVG is rejected because we don't bundle a renderer. The
/// frontend is unaware of this — a separate channel from `branding_svg`.
fn apply_window_icon(handle: &AppHandle, path: &str) -> std::result::Result<(), String> {
    let img = Image::from_path(path).map_err(|e| format!("read icon: {e}"))?;
    let win = handle.get_webview_window("main")
        .ok_or_else(|| "no 'main' window".to_string())?;
    win.set_icon(img).map_err(|e| format!("set_icon: {e}"))
}

/// Restore the bundled window icon (the one baked in by `tauri.conf.json`).
/// No-op when the default isn't available — happens during very early
/// boot, before the window is fully initialised.
fn restore_default_window_icon(handle: &AppHandle) {
    let Some(win) = handle.get_webview_window("main") else { return; };
    if let Some(default) = handle.default_window_icon() {
        let _ = win.set_icon(default.clone());
    }
}

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    install_set_branding(ctx, lua, ui)?;
    install_clear_branding(ctx, lua, ui)?;
    install_set_theme_tokens(ctx, lua, ui)?;
    install_clear_theme_tokens(ctx, lua, ui)?;
    Ok(())
}

fn install_set_branding(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, cfg: mlua::Table| {
        // Fields are optional but at least one logo-or-icon source must be
        // supplied — otherwise the call is a no-op the plugin author probably
        // didn't mean. Each surface updates independently: the SVG paints
        // in-app surfaces (title bar, welcome, About, HTML stats), the icon
        // path drives the OS window-icon API.
        //
        // `svg` and `svg_path` are mutually exclusive: pass the markup
        // inline OR an absolute path that the host reads off disk. The
        // path form is server-side fs (same trust model as
        // `window_icon_path`), so plugins don't need a `fs.read` perm to
        // ship their logo as a separate file.
        let mut svg:    Option<String> = cfg.get::<Option<String>>("svg").ok().flatten();
        let svg_path:   Option<String> = cfg.get::<Option<String>>("svg_path").ok().flatten();
        let icon_path:  Option<String> = cfg.get::<Option<String>>("window_icon_path").ok().flatten();

        if svg.is_some() && svg_path.is_some() {
            return Err(mlua::Error::RuntimeError(
                "arbor.ui.set_branding: pass either 'svg' or 'svg_path', not both".into()
            ));
        }
        if svg.is_none() && svg_path.is_none() && icon_path.is_none() {
            return Err(mlua::Error::RuntimeError(
                "arbor.ui.set_branding: at least one of 'svg', 'svg_path' or 'window_icon_path' is required".into()
            ));
        }

        if let Some(p) = svg_path.as_deref() {
            // Fail fast on a missing/unreadable file so the plugin author
            // sees the typo in the error stream instead of silently keeping
            // the previous mark.
            if !std::path::Path::new(p).is_file() {
                return Err(mlua::Error::RuntimeError(format!(
                    "arbor.ui.set_branding: 'svg_path' does not point to a file: {p}"
                )));
            }
            let body = std::fs::read_to_string(p).map_err(|e| mlua::Error::RuntimeError(format!(
                "arbor.ui.set_branding: failed to read 'svg_path' {p}: {e}"
            )))?;
            svg = Some(body);
        }

        if let Some(ref s) = svg {
            if !s.trim_start().starts_with("<svg") {
                return Err(mlua::Error::RuntimeError(
                    "arbor.ui.set_branding: SVG content must start with <svg".into()
                ));
            }
        }
        if let Some(ref p) = icon_path {
            // Fail fast on a missing file so the plugin author sees the
            // typo in the error stream instead of silently keeping the
            // previous icon. Tauri's set_icon also surfaces a useful
            // error, but doing this here keeps the message specific.
            if !std::path::Path::new(p).is_file() {
                return Err(mlua::Error::RuntimeError(format!(
                    "arbor.ui.set_branding: 'window_icon_path' does not point to a file: {p}"
                )));
            }
        }

        if let Some(ref h) = handle {
            // Apply the OS-level icon BEFORE writing the state — that way
            // a Tauri error still leaves the previous override intact.
            if let Some(ref p) = icon_path {
                if let Err(e) = apply_window_icon(h, p) {
                    return Err(mlua::Error::RuntimeError(format!(
                        "arbor.ui.set_branding: window_icon_path failed: {e}"
                    )));
                }
            }
            let state = h.state::<crate::AppState>();
            state.branding.apply(svg, icon_path, pname.clone());
            emit_branding_changed(h, &state.branding.snapshot());
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("set_branding", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_clear_branding(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, ()| {
        if let Some(ref h) = handle {
            let state = h.state::<crate::AppState>();
            // Only clear if WE own the override — protects against a noisy
            // plugin nuking another plugin's branding when it unloads.
            let Some(prev) = state.branding.clear(Some(&pname)) else { return Ok(()); };
            // If the previous state included a window icon, restore the
            // bundled default so the taskbar doesn't keep showing stale art.
            if prev.window_icon_path.is_some() {
                restore_default_window_icon(h);
            }
            emit_branding_changed(h, &state.branding.snapshot());
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("clear_branding", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_theme_tokens(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, cfg: mlua::Table| {
        let vars_tbl: mlua::Table = cfg.get("vars").map_err(|_| mlua::Error::RuntimeError(
            "arbor.ui.set_theme_tokens: 'vars' (table of --css-var = value) is required".into()
        ))?;
        let mut vars = serde_json::Map::new();
        for pair in vars_tbl.pairs::<String, String>() {
            let (k, v) = pair.map_err(|e| mlua::Error::RuntimeError(format!(
                "arbor.ui.set_theme_tokens: 'vars' entries must be string=string ({e})"
            )))?;
            if !k.starts_with("--") {
                return Err(mlua::Error::RuntimeError(format!(
                    "arbor.ui.set_theme_tokens: var '{k}' must start with '--' (CSS custom property)"
                )));
            }
            vars.insert(k, serde_json::Value::String(v));
        }
        if let Some(ref h) = handle {
            emit_theme_overlay(h, &pname, &serde_json::Value::Object(vars));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("set_theme_tokens", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_clear_theme_tokens(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, ()| {
        if let Some(ref h) = handle {
            // Empty-vars payload is the agreed-upon "release my overlay"
            // signal — frontend deletes the entry keyed by plugin name.
            emit_theme_overlay(h, &pname, &serde_json::Value::Object(serde_json::Map::new()));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("clear_theme_tokens", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
