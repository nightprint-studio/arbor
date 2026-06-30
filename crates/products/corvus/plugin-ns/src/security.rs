//! `arbor.security` (read-only git-provider security dashboard access), ported to
//! run through an [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/security.rs`: same namespace (`arbor.security`), same function names
//! (`supports` / `summary` / `findings` / `refresh_active_tab`), same argument
//! shapes (table-config opts), same `(value, err)` tuple conventions, same
//! permission-gate `RuntimeError` strings (`arbor.security.<op>: requires
//! provider = "read" (or higher)`), and same `arbor.security.<op>: …` /
//! `… resolve: …` / `… encode: …` / `… to_value: …` error prefixes.
//!
//! The only difference is *where the work goes*:
//!
//!   · the shell resolved the repo via `resolve_repo_path` (workspace registry
//!     `repo_id`, else the active tab) + `git_provider::provider_for_path`, then
//!     called `GitProvider::{supports_security, fetch_security_summary,
//!     fetch_security_findings}` directly (`block_on`-wrapped, async);
//!   · here the installer reads the active repo path from the
//!     `__arbor_current_repo__` Lua global (the same active-repo path the rest of
//!     the ported namespaces read) and forwards it together with the optional
//!     `repo_id`; the captured `Arc<dyn NsHost>` does the registry resolution,
//!     provider lookup and the same async provider calls (the host owns the tokio
//!     runtime / `block_on`), so behaviour and error strings match.
//!
//! ## Filter marshalling
//!
//! `findings` accepts the same lowercase token filters as the shell
//! (`severities` / `states` / `report_types` string arrays + `search` / `limit`).
//! To keep this crate free of the provider's `Severity` / `FindingState` enums,
//! the *raw token strings* are passed straight through to the host, which parses
//! them exactly as the shell did (unknown tokens dropped; empty `states` →
//! `["detected","confirmed"]` active-scope default). The installer therefore does
//! **not** parse or default the tokens — that lives in the host so the parse table
//! stays in one place next to the enum.
//!
//! ## Null sentinel
//!
//! `summary` / `refresh_active_tab` carry `time_series: Option<_>` (None for
//! GitHub). The shell stripped mlua's null sentinel via
//! `SerializeOptions::serialize_none_to_null(false)` so defensive `tbl and
//! tbl.field` chains in Lua see plain `nil`, not a userdata. That same option is
//! applied here ([`lua_value_no_null`]) for every JSON-returning op, byte-for-byte.
//!
//! Requires `provider = "read"` (or higher) for every op.

use mlua::{Lua, LuaSerdeExt, SerializeOptions, Table};

use arbor_plugin_core::prelude::{
    err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError, PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// Read the active repo path from the `__arbor_current_repo__` Lua global. `None`
/// when no repo is active — the host then surfaces the shell's "no active tab"
/// error (or, for an explicit `repo_id`, resolves it from the registry instead).
fn current_repo(lua: &Lua) -> Option<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
}

/// Serialize JSON to a Lua value with mlua's null sentinel suppressed, exactly as
/// the shell's `lua_value_no_null` did: `None` / `Value::Null` reach Lua as plain
/// `nil` (not the null-sentinel userdata), so defensive `tbl and tbl.field`
/// chains don't index a userdata and crash. The security summary's
/// `time_series: Option<_>` (None on GitHub) is the case this guards.
fn lua_value_no_null(lua: &Lua, v: &serde_json::Value) -> mlua::Result<mlua::Value> {
    let opts = SerializeOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false);
    lua.to_value_with(v, opts)
}

/// Pull an optional string-array from a Lua opts table, dropping non-string
/// elements (mirrors the shell's `pull_string_array`). Empty when the key is
/// absent or not a table.
fn pull_string_array(opts: &Table, key: &str) -> Vec<String> {
    opts.get::<Option<Table>>(key)
        .ok()
        .flatten()
        .map(|t| {
            t.sequence_values::<String>()
                .filter_map(|v| v.ok())
                .collect()
        })
        .unwrap_or_default()
}

/// `arbor.security.*` installer. Holds the host handle the closures call through.
pub struct SecurityInstaller {
    host: NsHostHandle,
}

impl SecurityInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for SecurityInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let sec_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_supports(self.host.clone(), ctx, lua, &sec_table)?;
        install_summary(self.host.clone(), ctx, lua, &sec_table)?;
        install_findings(self.host.clone(), ctx, lua, &sec_table)?;
        install_refresh_active_tab(self.host.clone(), ctx, lua, &sec_table)?;

        arbor
            .set("security", sec_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

/// `arbor.security.supports({ repo_id? }) → (bool, nil) | (nil, err)`
///
/// Cheap probe. `false` (NOT an error) for repos with no provider remote, no
/// stored token, or where the provider doesn't expose the dashboard for the
/// current account — the host returns `false` for the "no provider for this
/// remote" case, matching the shell's early `ok2(false)`.
fn install_supports(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    sec: &Table,
) -> PluginCoreResult<()> {
    let provider_read = ctx.provider_read;
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
            if !provider_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.security.supports: requires provider = \"read\" (or higher)".to_string(),
                ));
            }
            let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
            let repo_id: Option<String> = opts.get::<Option<String>>("repo_id").ok().flatten();
            let current = current_repo(lua_ctx);

            match host.security_supports(repo_id.as_deref(), current.as_deref()) {
                Ok(supported) => ok2(lua_ctx, mlua::Value::Boolean(supported)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    sec.set("supports", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// `arbor.security.summary({ repo_id?, range_days? }) → (SecuritySummary, nil) | (nil, err)`
///
/// `range_days` defaults to 30, clamped to `[7, 90]`. The clamp+default is done
/// here (matching where the shell did it) before the value reaches the host.
fn install_summary(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    sec: &Table,
) -> PluginCoreResult<()> {
    let provider_read = ctx.provider_read;
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
            if !provider_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.security.summary: requires provider = \"read\" (or higher)".to_string(),
                ));
            }
            let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
            let repo_id: Option<String> = opts.get::<Option<String>>("repo_id").ok().flatten();
            let range_days: u32 = opts
                .get::<Option<u32>>("range_days")
                .ok()
                .flatten()
                .map(|v| v.clamp(7, 90))
                .unwrap_or(30);
            let current = current_repo(lua_ctx);

            let json = match host.security_summary(
                repo_id.as_deref(),
                current.as_deref(),
                range_days,
            ) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_value_no_null(lua_ctx, &json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("arbor.security.summary to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    sec.set("summary", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// `arbor.security.findings({ repo_id?, severities?, states?, report_types?,
///                            search?, limit? }) → ([SecurityFinding], nil) | (nil, err)`
///
/// Raw lowercase token arrays (`severities` / `states` / `report_types`) are
/// passed through to the host, which parses them into the provider enums exactly
/// as the shell did (unknown tokens dropped; empty `states` → active-scope
/// `["detected","confirmed"]` default). `search` / `limit` are optional scalars.
fn install_findings(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    sec: &Table,
) -> PluginCoreResult<()> {
    let provider_read = ctx.provider_read;
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
            if !provider_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.security.findings: requires provider = \"read\" (or higher)".to_string(),
                ));
            }
            let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
            let repo_id: Option<String> = opts.get::<Option<String>>("repo_id").ok().flatten();
            let search: Option<String> = opts.get::<Option<String>>("search").ok().flatten();
            let limit: Option<u32> = opts.get::<Option<u32>>("limit").ok().flatten();

            let severities = pull_string_array(&opts, "severities");
            let states = pull_string_array(&opts, "states");
            let report_types = pull_string_array(&opts, "report_types");
            let current = current_repo(lua_ctx);

            let json = match host.security_findings(
                repo_id.as_deref(),
                current.as_deref(),
                &severities,
                &states,
                &report_types,
                search.as_deref(),
                limit,
            ) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_value_no_null(lua_ctx, &json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("arbor.security.findings to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    sec.set("findings", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// `arbor.security.refresh_active_tab({ range_days? }) → (SecuritySummary, nil) | (nil, err)`
///
/// Like `summary({})` for the active repo, plus the host emits
/// `arbor://security-refresh { tab_id, summary }` so the frontend's
/// `securityStore` swaps in the fresh data without its own IPC. The active repo
/// is the `__arbor_current_repo__` path (no `repo_id` opt — the point is "refresh
/// whatever the user is looking at"); the host resolves that path to the open
/// tab id for the emit payload and surfaces "no active tab" when absent.
fn install_refresh_active_tab(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    sec: &Table,
) -> PluginCoreResult<()> {
    let provider_read = ctx.provider_read;
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
            if !provider_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.security.refresh_active_tab: requires provider = \"read\" (or higher)"
                        .to_string(),
                ));
            }
            let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
            let range_days: u32 = opts
                .get::<Option<u32>>("range_days")
                .ok()
                .flatten()
                .map(|v| v.clamp(7, 90))
                .unwrap_or(30);
            let current = current_repo(lua_ctx);

            let json = match host.security_refresh_active_tab(current.as_deref(), range_days) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_value_no_null(lua_ctx, &json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(
                    lua_ctx,
                    format!("arbor.security.refresh_active_tab to_value: {e}"),
                ),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    sec.set("refresh_active_tab", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
