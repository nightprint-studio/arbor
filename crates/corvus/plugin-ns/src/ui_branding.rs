//! `arbor.ui.{set_branding, clear_branding, set_theme_tokens, clear_theme_tokens}`,
//! ported to run through an [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/ui/branding.rs`: it attaches the same four functions
//! (`set_branding` / `clear_branding` / `set_theme_tokens` /
//! `clear_theme_tokens`) onto the existing `arbor.ui` table, same argument
//! shapes (config tables / no-arg), same `RuntimeError` strings, and the same
//! RAM-only semantics.
//!
//! This is a **PROXY** namespace: the window-icon API is Tauri-only and the
//! `AppState.branding` store + `arbor://*` rebroadcast live in the shell, not in
//! `corvus-be`. So the side-effecting half of each op goes through the captured
//! `Arc<dyn NsHost>`, whose `corvus-be` impl calls back over the reverse channel
//! (`host_call("__set_branding" | "__clear_branding" | "__set_theme_overlay" |
//! "__clear_theme_overlay", …)`); the matching shell handlers in
//! `src-tauri/src/ipc/mod.rs` apply the OS window-icon, write `AppState.branding`
//! and emit `arbor://branding-changed` / `arbor://theme-overlay` exactly as
//! `ns_shell/ui/branding.rs` did.
//!
//! ## What stays installer-side vs host-side
//!
//! Every piece of validation that is **pure** (no AppState, no Tauri handle) is
//! kept installer-side so the raise-on-bad-shape behaviour is byte-for-byte
//! identical to the shell, even across the JSON boundary:
//!   · `svg` / `svg_path` mutual-exclusion + "at least one source" checks,
//!   · `svg_path` is-file + read-to-string (server-side fs, same trust model as
//!     the shell — no `fs.read` perm needed),
//!   · the `<svg` content check,
//!   · the `window_icon_path` is-file check,
//!   · the `set_theme_tokens` `vars` table → `{--k = v}` map build + `--` prefix
//!     check.
//! The host then receives the already-resolved values (the inline SVG body, the
//! icon path, the plugin name, the vars object) and performs only the
//! AppState/Tauri/emit side-effects.
//!
//! ## `arbor.ui` table must already exist
//!
//! Like the shell installer, this attaches onto the `arbor.ui` table created by
//! plugin-core's `ns::ui::install`. If `arbor.ui` is missing the install fails
//! with the same diagnostic the shell produced.
//!
//! Calling convention (unchanged from the shell — see `ns_shell/ui/branding.rs`):
//!   · `set_branding{svg? | svg_path?, window_icon_path?}` — config table, raises
//!     on a bad/empty source, returns nothing on success.
//!   · `clear_branding()` — no args; only clears when this plugin owns the
//!     override.
//!   · `set_theme_tokens{vars = {["--x"]=v, …}}` — config table, raises when
//!     `vars` is missing/non-string or a key lacks the `--` prefix.
//!   · `clear_theme_tokens()` — no args; releases this plugin's overlay.
//! All four are RAM-only (no perm gate in the shell — none added here).

use mlua::{Lua, Table};

use arbor_plugin_core::prelude::{ApiCtx, LuaNamespaceInstaller, PluginCoreError, PluginCoreResult};

use crate::nshost::NsHostHandle;

/// `arbor.ui.{set,clear}_branding` + `arbor.ui.{set,clear}_theme_tokens`
/// installer. Attaches onto the pre-existing `arbor.ui` table (created by
/// plugin-core's `ns::ui`), holding the host handle the closures call through.
pub struct UiBrandingInstaller {
    host: NsHostHandle,
}

impl UiBrandingInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for UiBrandingInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        // Attach onto the existing `arbor.ui` table (plugin-core's `ns::ui`
        // installs it first; the installer ordering preserves that). Same
        // diagnostic the shell produced when the table is absent.
        let ui: Table = arbor.get("ui").map_err(|e| {
            PluginCoreError::Plugin(format!(
                "arbor.ui.branding install: arbor.ui table missing (plugin-core ns::ui \
                 must install first): {e}"
            ))
        })?;

        install_set_branding(self.host.clone(), ctx, lua, &ui)?;
        install_clear_branding(self.host.clone(), ctx, lua, &ui)?;
        install_set_theme_tokens(self.host.clone(), ctx, lua, &ui)?;
        install_clear_theme_tokens(self.host.clone(), ctx, lua, &ui)?;
        Ok(())
    }
}

fn install_set_branding(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    ui: &Table,
) -> PluginCoreResult<()> {
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |_, cfg: mlua::Table| {
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
            let mut svg: Option<String> = cfg.get::<Option<String>>("svg").ok().flatten();
            let svg_path: Option<String> = cfg.get::<Option<String>>("svg_path").ok().flatten();
            let icon_path: Option<String> =
                cfg.get::<Option<String>>("window_icon_path").ok().flatten();

            if svg.is_some() && svg_path.is_some() {
                return Err(mlua::Error::RuntimeError(
                    "arbor.ui.set_branding: pass either 'svg' or 'svg_path', not both".into(),
                ));
            }
            if svg.is_none() && svg_path.is_none() && icon_path.is_none() {
                return Err(mlua::Error::RuntimeError(
                    "arbor.ui.set_branding: at least one of 'svg', 'svg_path' or 'window_icon_path' is required".into(),
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
                let body = std::fs::read_to_string(p).map_err(|e| {
                    mlua::Error::RuntimeError(format!(
                        "arbor.ui.set_branding: failed to read 'svg_path' {p}: {e}"
                    ))
                })?;
                svg = Some(body);
            }

            if let Some(ref s) = svg {
                if !s.trim_start().starts_with("<svg") {
                    return Err(mlua::Error::RuntimeError(
                        "arbor.ui.set_branding: SVG content must start with <svg".into(),
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

            // The OS window-icon apply + AppState.branding write + emit live in
            // the shell. The host applies the icon BEFORE writing the state so a
            // Tauri error still leaves the previous override intact (the shell
            // handler preserves that ordering). The `window_icon_path failed: …`
            // RuntimeError is re-raised host-side and surfaced verbatim.
            host.ui_set_branding(svg.as_deref(), icon_path.as_deref(), &pname)
                .map_err(mlua::Error::RuntimeError)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ui.set("set_branding", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_clear_branding(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    ui: &Table,
) -> PluginCoreResult<()> {
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |_, ()| {
            // Only clears if WE own the override (the shell guards this), and
            // restores the bundled window icon if the cleared state carried one.
            host.ui_clear_branding(&pname)
                .map_err(mlua::Error::RuntimeError)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ui.set("clear_branding", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_theme_tokens(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    ui: &Table,
) -> PluginCoreResult<()> {
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |_, cfg: mlua::Table| {
            let vars_tbl: mlua::Table = cfg.get("vars").map_err(|_| {
                mlua::Error::RuntimeError(
                    "arbor.ui.set_theme_tokens: 'vars' (table of --css-var = value) is required"
                        .into(),
                )
            })?;
            let mut vars = serde_json::Map::new();
            for pair in vars_tbl.pairs::<String, String>() {
                let (k, v) = pair.map_err(|e| {
                    mlua::Error::RuntimeError(format!(
                        "arbor.ui.set_theme_tokens: 'vars' entries must be string=string ({e})"
                    ))
                })?;
                if !k.starts_with("--") {
                    return Err(mlua::Error::RuntimeError(format!(
                        "arbor.ui.set_theme_tokens: var '{k}' must start with '--' (CSS custom property)"
                    )));
                }
                vars.insert(k, serde_json::Value::String(v));
            }
            // Theme overlays live entirely on the frontend — the shell just
            // rebroadcasts via `arbor://theme-overlay`. Forward the assembled
            // vars object to the host for that emit.
            host.ui_set_theme_overlay(&pname, serde_json::Value::Object(vars))
                .map_err(mlua::Error::RuntimeError)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ui.set("set_theme_tokens", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_clear_theme_tokens(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    ui: &Table,
) -> PluginCoreResult<()> {
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |_, ()| {
            // Empty-vars payload is the agreed-upon "release my overlay" signal —
            // the frontend deletes the entry keyed by plugin name. The shell
            // emits `arbor://theme-overlay` with an empty object.
            host.ui_clear_theme_overlay(&pname)
                .map_err(mlua::Error::RuntimeError)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ui.set("clear_theme_tokens", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
