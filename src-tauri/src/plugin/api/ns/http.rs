//! `arbor.http.get(url, [opts,] callback)`.
//!
//! Native HTTP client (reqwest) so plugins don't have to shell out via
//! curl. The call returns immediately; the response is delivered to the
//! Lua callback as a single table:
//!   { ok: bool, status: number, body: string, error?: string }
//!
//! opts is an optional table:
//!   { headers = { Foo = "bar" }, timeout_ms = 10000 }
//!
//! Permission gate: the plugin's `permissions.network` must contain "*"
//! OR the URL's host (exact match or registrable suffix — `search.maven.org`
//! matches `maven.org` and itself).
//!
//! Threading: the callback is dispatched via the same `__arbor_hooks__`
//! table and `fire_hook_on()` plumbing as `arbor.job.spawn`'s on_done,
//! so it shares the per-plugin Lua-VM mutex semantics for free.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Lua, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::http_worker::perform_http_get;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let http_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;
    let pname = ctx.plugin_name.clone();
    let net_perm = ctx.network_perm.clone();
    let handle   = ctx.app_handle.clone();

    // Per-plugin atomic counter for synthetic callback names. Same pattern
    // as `__job_done_<id>__` so concurrent in-flight requests can't
    // overwrite each other's callback registration.
    let counter = Arc::new(AtomicU64::new(0));

    let get_fn = lua
        .create_function(move |lua_ctx, args: mlua::MultiValue| {
            let (url, opts, callback) = parse_get_args(args)?;
            permission_gate(&pname, &net_perm, &url)?;
            let (headers_vec, timeout_ms) = parse_opts(opts);

            // Reserve a unique synthetic hook name for this request and
            // park the callback under it. The async task fires that hook
            // by name once the HTTP response lands.
            let req_n = counter.fetch_add(1, Ordering::Relaxed);
            let hook_name = format!("__http_resp_{}_{}__", pname, req_n);
            let registry: Table = lua_ctx.globals().get("__arbor_hooks__")?;
            let list = lua_ctx.create_table()?;
            list.push(callback)?;
            registry.set(hook_name.clone(), list)?;

            // Dispatch the actual HTTP call on Tauri's tokio runtime.
            let Some(ref h) = handle else {
                return Err(mlua::Error::RuntimeError(
                    "arbor.http.get: app handle unavailable".into()));
            };
            let h           = h.clone();
            let pname_owned = pname.clone();
            let url_owned   = url.clone();
            let hook_owned  = hook_name.clone();

            tauri::async_runtime::spawn(async move {
                let payload = perform_http_get(
                    &url_owned, &headers_vec, timeout_ms,
                ).await;
                let payload_str = serde_json::to_string(&payload)
                    .unwrap_or_else(|_| "{\"ok\":false,\"status\":0,\"body\":\"\",\"error\":\"serialise failed\"}".into());

                // Re-enter the plugin's Lua VM through the host mutex
                // and fire the synthetic hook. The entry stays in
                // __arbor_hooks__ — it's a single function reference
                // and the modal use-case fires only a handful of these
                // per analysis. Same pattern as `__job_done_*__` from
                // arbor.job.spawn.
                let state = h.state::<crate::AppState>();
                if let Ok(host) = state.plugin_host.lock() {
                    let _ = host.fire_hook_on(&pname_owned, &hook_owned, &payload_str);
                };
            });

            Ok(())
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    http_table.set("get", get_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    arbor.set("http", http_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── Argument parsing ────────────────────────────────────────────────────

fn parse_get_args(
    args: mlua::MultiValue,
) -> mlua::Result<(String, Option<mlua::Table>, mlua::Function)> {
    let mut iter = args.into_iter();
    let url_v = iter.next()
        .ok_or_else(|| mlua::Error::RuntimeError("arbor.http.get: url required".into()))?;
    let url: String = match url_v {
        mlua::Value::String(s) => s.to_str()
            .map(|c| c.to_string())
            .map_err(|_| mlua::Error::RuntimeError("arbor.http.get: url must be utf-8".into()))?,
        _ => return Err(mlua::Error::RuntimeError(
            "arbor.http.get: url must be a string".into())),
    };

    let (opts, callback): (Option<mlua::Table>, mlua::Function) = match (iter.next(), iter.next()) {
        (Some(mlua::Value::Function(cb)), None) => (None, cb),
        (Some(mlua::Value::Table(t)), Some(mlua::Value::Function(cb))) => (Some(t), cb),
        _ => return Err(mlua::Error::RuntimeError(
            "arbor.http.get: expected (url, callback) or (url, opts, callback)".into())),
    };

    Ok((url, opts, callback))
}

fn permission_gate(pname: &str, net_perm: &[String], url: &str) -> mlua::Result<()> {
    if net_perm.is_empty() {
        return Err(mlua::Error::RuntimeError(format!(
            "arbor.http.get: '{}' requires `network` permission. \
             Add to plugin.toml: network = [\"<host>\"] (or [\"*\"]).",
            pname)));
    }
    // Extract host: split on `://`, then on `/`, `:`, `?`, `#`.
    let host = url.split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split(|c: char| c == '/' || c == ':' || c == '?' || c == '#')
        .next()
        .unwrap_or("")
        .to_string();
    if host.is_empty() {
        return Err(mlua::Error::RuntimeError(
            format!("arbor.http.get: cannot parse host from URL '{url}'")));
    }
    let allowed = net_perm.iter().any(|h| {
        h == "*"
        || h == &host
        || (host.ends_with(&format!(".{h}")))
    });
    if !allowed {
        return Err(mlua::Error::RuntimeError(format!(
            "arbor.http.get: host '{host}' not in plugin's network allowlist {:?}",
            net_perm)));
    }
    Ok(())
}

fn parse_opts(opts: Option<mlua::Table>) -> (Vec<(String, String)>, u64) {
    let mut headers_vec: Vec<(String, String)> = Vec::new();
    let mut timeout_ms: u64 = 10_000;
    if let Some(opts_t) = opts {
        if let Ok(htbl) = opts_t.get::<mlua::Table>("headers") {
            for pair in htbl.pairs::<String, String>() {
                if let Ok((k, v)) = pair { headers_vec.push((k, v)); }
            }
        }
        if let Ok(t) = opts_t.get::<u64>("timeout_ms") { timeout_ms = t; }
    }
    (headers_vec, timeout_ms)
}
