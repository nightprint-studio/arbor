//! `arbor.security` — read-only git-provider security dashboard access.
//!
//! Wraps `GitProvider::{supports_security, fetch_security_summary,
//! fetch_security_findings}` so plugins can read posture data for any
//! registered repository without ever touching the OAuth token. Same
//! permission gate as `arbor.mr.*` / `arbor.ci.*`: requires `provider = "read"`.
//!
//! Calling convention:
//!   · `(value, nil)` on success, `(nil, err)` on recoverable failure.
//!   · Permission denied raises a Lua error.
//!   · `repo_id` defaults to the active tab when omitted (matches the rest
//!     of the provider-namespaced APIs).

use mlua::{Lua, LuaSerdeExt, SerializeOptions, Table};
use tauri::{Emitter, Manager};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};

// `Option::None` / `serde_json::Value::Null` should reach Lua as plain `nil`,
// not mlua's null-sentinel userdata — otherwise plugins that defensively
// chain `tbl and tbl.field` end up indexing a userdata and crashing. The
// security summary returns `time_series: Option<_>` (None for GitHub) so
// every consumer hit this until we strip the sentinel here.
fn lua_value_no_null(lua: &Lua, v: &serde_json::Value) -> mlua::Result<mlua::Value> {
    let opts = SerializeOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false);
    lua.to_value_with(v, opts)
}

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let sec_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_supports(ctx, lua, &sec_table)?;
    install_summary(ctx, lua, &sec_table)?;
    install_findings(ctx, lua, &sec_table)?;
    install_refresh_active_tab(ctx, lua, &sec_table)?;

    arbor.set("security", sec_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

macro_rules! block_on_provider {
    ($fut:expr) => {{
        let rt = tokio::runtime::Handle::try_current().ok();
        if let Some(h) = rt {
            h.block_on($fut)
        } else {
            match tokio::runtime::Runtime::new() {
                Ok(r)  => r.block_on($fut),
                Err(e) => Err(crate::git_provider::types::error::ProviderError::Internal(
                    format!("runtime: {e}"),
                )),
            }
        }
    }};
}

/// `arbor.security.supports({ repo_id? }) → (bool, nil) | (nil, err)`
///
/// Cheap probe (single GraphQL/REST round-trip) — `false` for repos with
/// no provider remote, no stored token, or where the provider doesn't
/// expose the dashboard for the current account (GitLab Free without
/// Ultimate, GitHub repo with GHAS off, etc.).
fn install_supports(ctx: &ApiCtx, lua: &Lua, sec: &Table) -> Result<()> {
    let provider_read = ctx.provider_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
        if !provider_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.security.supports: requires provider = \"read\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return err2(lua_ctx, "arbor.security.supports: app handle unavailable");
        };

        let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
        let repo_id: Option<String> = opts.get::<Option<String>>("repo_id").ok().flatten();

        let path = match super::mr::resolve_repo_path(h, repo_id.as_deref()) {
            Ok(p)  => p,
            Err(e) => return err2(lua_ctx, format!("arbor.security.supports: {e}")),
        };
        let state_app = h.state::<crate::AppState>();
        let resolved = match crate::git_provider::provider_for_path(&state_app, &path) {
            Ok(r)  => r,
            // No provider for this remote → not supported, NOT an error.
            Err(_) => return ok2(lua_ctx, mlua::Value::Boolean(false)),
        };
        let supported = match block_on_provider!(
            resolved.provider.supports_security(&resolved.repo)
        ) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.security.supports: {e}")),
        };
        ok2(lua_ctx, mlua::Value::Boolean(supported))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    sec.set("supports", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// `arbor.security.summary({ repo_id?, range_days? }) → (SecuritySummary, nil) | (nil, err)`
///
/// Returns the headline summary used by the dashboard panel: severity
/// counts (active findings only), median ages, optional risk score and
/// vulnerabilities-over-time series. `range_days` defaults to 30 and is
/// clamped to `[7, 90]`.
fn install_summary(ctx: &ApiCtx, lua: &Lua, sec: &Table) -> Result<()> {
    let provider_read = ctx.provider_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
        if !provider_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.security.summary: requires provider = \"read\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return err2(lua_ctx, "arbor.security.summary: app handle unavailable");
        };

        let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
        let repo_id:    Option<String> = opts.get::<Option<String>>("repo_id").ok().flatten();
        let range_days: u32 = opts.get::<Option<u32>>("range_days")
            .ok().flatten()
            .map(|v| v.clamp(7, 90))
            .unwrap_or(30);

        let path = match super::mr::resolve_repo_path(h, repo_id.as_deref()) {
            Ok(p)  => p,
            Err(e) => return err2(lua_ctx, format!("arbor.security.summary: {e}")),
        };
        let state_app = h.state::<crate::AppState>();
        let resolved = match crate::git_provider::provider_for_path(&state_app, &path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("arbor.security.summary resolve: {e}")),
        };
        let summary = match block_on_provider!(
            resolved.provider.fetch_security_summary(&resolved.repo, range_days)
        ) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.security.summary: {e}")),
        };

        let json = match serde_json::to_value(&summary) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.security.summary encode: {e}")),
        };
        match lua_value_no_null(lua_ctx, &json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("arbor.security.summary to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    sec.set("summary", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// `arbor.security.findings({ repo_id?, severities?, states?, report_types?,
///                            search?, limit? }) → ([SecurityFinding], nil) | (nil, err)`
///
/// All filters are optional. `states` defaults to `{"detected", "confirmed"}`
/// (active scope) — pass `{"resolved", "dismissed"}` to inspect closed
/// findings, or `{"detected","confirmed","resolved","dismissed"}` for both.
/// `severities` accepts the same lowercase tokens used by the host:
/// `"critical" | "high" | "medium" | "low" | "info" | "unknown"`.
fn install_findings(ctx: &ApiCtx, lua: &Lua, sec: &Table) -> Result<()> {
    use crate::git_provider::types::{Severity, FindingState, SecurityFilters};

    fn parse_severity(s: &str) -> Option<Severity> {
        match s.to_ascii_lowercase().as_str() {
            "critical" => Some(Severity::Critical),
            "high"     => Some(Severity::High),
            "medium"   => Some(Severity::Medium),
            "low"      => Some(Severity::Low),
            "info"     => Some(Severity::Info),
            "unknown"  => Some(Severity::Unknown),
            _ => None,
        }
    }
    fn parse_state(s: &str) -> Option<FindingState> {
        match s.to_ascii_lowercase().as_str() {
            "detected"  => Some(FindingState::Detected),
            "confirmed" => Some(FindingState::Confirmed),
            "resolved"  => Some(FindingState::Resolved),
            "dismissed" => Some(FindingState::Dismissed),
            _ => None,
        }
    }
    fn pull_string_array(opts: &Table, key: &str) -> Vec<String> {
        opts.get::<Option<Table>>(key)
            .ok()
            .flatten()
            .map(|t| t.sequence_values::<String>().filter_map(|v| v.ok()).collect())
            .unwrap_or_default()
    }

    let provider_read = ctx.provider_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
        if !provider_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.security.findings: requires provider = \"read\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return err2(lua_ctx, "arbor.security.findings: app handle unavailable");
        };

        let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
        let repo_id: Option<String> = opts.get::<Option<String>>("repo_id").ok().flatten();
        let search:  Option<String> = opts.get::<Option<String>>("search").ok().flatten();
        let limit:   Option<u32>    = opts.get::<Option<u32>>("limit").ok().flatten();

        let severities: Vec<Severity> = pull_string_array(&opts, "severities")
            .iter()
            .filter_map(|s| parse_severity(s))
            .collect();
        // Default state filter mirrors the dashboard: active only. Empty
        // input AFTER an explicit empty array would still be treated as
        // "active" — plugins that want everything must enumerate the four
        // states explicitly.
        let states_in = pull_string_array(&opts, "states");
        let states: Vec<FindingState> = if states_in.is_empty() {
            vec![FindingState::Detected, FindingState::Confirmed]
        } else {
            states_in.iter().filter_map(|s| parse_state(s)).collect()
        };
        let report_types: Vec<String> = pull_string_array(&opts, "report_types");

        let path = match super::mr::resolve_repo_path(h, repo_id.as_deref()) {
            Ok(p)  => p,
            Err(e) => return err2(lua_ctx, format!("arbor.security.findings: {e}")),
        };
        let state_app = h.state::<crate::AppState>();
        let resolved = match crate::git_provider::provider_for_path(&state_app, &path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("arbor.security.findings resolve: {e}")),
        };

        let filters = SecurityFilters {
            severities,
            states,
            report_types,
            search,
            limit,
        };
        let findings = match block_on_provider!(
            resolved.provider.fetch_security_findings(&resolved.repo, filters)
        ) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.security.findings: {e}")),
        };

        let json = match serde_json::to_value(&findings) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.security.findings encode: {e}")),
        };
        match lua_value_no_null(lua_ctx, &json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("arbor.security.findings to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    sec.set("findings", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// `arbor.security.refresh_active_tab({ range_days? }) → (SecuritySummary, nil) | (nil, err)`
///
/// Like `arbor.security.summary({})` but two extras:
///   1. Resolves the active tab from `AppState` (no `repo_id` opt — the
///      whole point is "refresh whatever the user is looking at").
///   2. Emits `arbor://security-refresh { tab_id, summary }` so the
///      frontend's `securityStore` can swap in the fresh data without
///      issuing its own IPC. Mirrors the `arbor.repo.fetch_active_tab` →
///      `arbor://graph-refresh` contract used by the auto-fetch plugin.
///
/// Returns the same Lua table shape as `summary` so plugins can compute
/// deltas in the same call (e.g. compare new counts against a snapshot).
fn install_refresh_active_tab(ctx: &ApiCtx, lua: &Lua, sec: &Table) -> Result<()> {
    let provider_read = ctx.provider_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
        if !provider_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.security.refresh_active_tab: requires provider = \"read\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return err2(lua_ctx, "arbor.security.refresh_active_tab: app handle unavailable");
        };

        let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
        let range_days: u32 = opts.get::<Option<u32>>("range_days")
            .ok().flatten()
            .map(|v| v.clamp(7, 90))
            .unwrap_or(30);

        let state_app = h.state::<crate::AppState>();
        let tab_id = {
            let lock = state_app.active_tab_id.lock()
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            lock.clone()
        };
        let Some(tab_id) = tab_id else {
            return err2(lua_ctx, "arbor.security.refresh_active_tab: no active tab");
        };

        // Resolve via tab id (not path) so we get the same provider the UI
        // uses and self-hosted GitLab instances are auto-registered when
        // missing — same path as the Tauri command.
        let resolved = match crate::git_provider::provider_for_tab(&state_app, &tab_id) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("arbor.security.refresh_active_tab resolve: {e}")),
        };
        let summary = match block_on_provider!(
            resolved.provider.fetch_security_summary(&resolved.repo, range_days)
        ) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.security.refresh_active_tab: {e}")),
        };

        // Notify the frontend store with the fresh summary. Fire-and-forget
        // — the plugin still gets the value back via the return path.
        let _ = h.emit("arbor://security-refresh", serde_json::json!({
            "tab_id":  &tab_id,
            "summary": &summary,
        }));

        let json = match serde_json::to_value(&summary) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.security.refresh_active_tab encode: {e}")),
        };
        match lua_value_no_null(lua_ctx, &json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("arbor.security.refresh_active_tab to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    sec.set("refresh_active_tab", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
