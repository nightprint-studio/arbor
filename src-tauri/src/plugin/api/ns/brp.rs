//! `arbor.brp.*` — Bevy Remote Protocol surface for plugins.
//!
//! Phase 1 — HTTP JSON-RPC:
//!   - `arbor.brp.connect({endpoint?, timeout_ms?}, callback)`
//!   - `arbor.brp.disconnect()`
//!   - `arbor.brp.status() → { connected, endpoint?, connected_at? }`
//!   - `arbor.brp.call(method, params?, callback)`
//!
//! Phase 2 — SSE watch streams:
//!   - `arbor.brp.watch(method, params?, callback) → sub_id`
//!   - `arbor.brp.unwatch(sub_id)`
//!
//! Permission gate: plugins must declare a `network` allowlist that matches
//! the BRP endpoint's host. The `bevy-brp` plugin shipped with Arbor uses
//! `["127.0.0.1", "localhost"]` to cover the canonical loopback. Non-loopback
//! endpoints require a deliberate allowlist entry — Phase 1 doesn't yet pop
//! the "RCE warning" modal (that's a host-UI concern, Phase 2 hardening).
//!
//! Single-shot callback envelope (mirrors `arbor.http.get`):
//!   `{ ok = true,  result = <json> }`
//!   `{ ok = false, error = { kind, message, code?, data? } }`
//! `kind` is one of "transport" | "status" | "invalid_response" | "rpc" |
//! "not_connected" | "internal" | "permission" — the Lua side branches on
//! `payload.ok` plus optional `payload.error.kind` for differential handling.
//!
//! Watch callback envelope (fires repeatedly until `unwatch` or the stream
//! ends):
//!   `{ ok = true,  event = "open" }`
//!   `{ ok = true,  event = "data",  result = <json> }`
//!   `{ ok = true,  event = "close" }`
//!   `{ ok = false, event = "error", error = { kind, message, code?, data? } }`
//! The `event` discriminator lets the plugin distinguish "connection up",
//! "payload arrived", and "stream gone" without inspecting `ok` alone.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Manager;

use crate::brp::{
    BrpClient, BrpSession, BrpStatus, DEFAULT_ENDPOINT, WatchEvent, WatchSub, probe_capabilities,
    run_watch_stream,
};
use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let brp_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;
    let counter = Arc::new(AtomicU64::new(0));

    install_connect(ctx, lua, &brp_table, counter.clone())?;
    install_disconnect(ctx, lua, &brp_table)?;
    install_status(ctx, lua, &brp_table)?;
    install_call(ctx, lua, &brp_table, counter.clone())?;
    install_watch(ctx, lua, &brp_table, counter)?;
    install_unwatch(ctx, lua, &brp_table)?;

    arbor.set("brp", brp_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── connect ─────────────────────────────────────────────────────────────

fn install_connect(
    ctx: &ApiCtx,
    lua: &Lua,
    brp_table: &Table,
    counter: Arc<AtomicU64>,
) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let net_perm = ctx.network_perm.clone();
    let handle = ctx.app_handle.clone();

    let fn_ = lua
        .create_function(move |lua_ctx, args: mlua::MultiValue| {
            let (opts, callback) = parse_opts_and_callback(args)?;
            let endpoint = opts
                .as_ref()
                .and_then(|t| t.get::<String>("endpoint").ok())
                .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
            let timeout_ms = opts
                .as_ref()
                .and_then(|t| t.get::<u64>("timeout_ms").ok())
                .unwrap_or(DEFAULT_TIMEOUT_MS);

            if let Err(msg) = permission_gate(&pname, &net_perm, &endpoint) {
                fire_error_inline(lua_ctx, &pname, callback, "permission", &msg)?;
                return Ok(());
            }

            let Some(ref h) = handle else {
                return Err(mlua::Error::RuntimeError(
                    "arbor.brp.connect: app handle unavailable".into(),
                ));
            };

            let hook_name = reserve_callback(lua_ctx, &pname, &counter, "connect", callback)?;
            let h = h.clone();
            let pname_owned = pname.clone();
            let endpoint_owned = endpoint.clone();
            let hook_owned = hook_name;

            tauri::async_runtime::spawn(async move {
                let payload = perform_connect(&h, endpoint_owned, timeout_ms).await;
                fire_callback(&h, &pname_owned, &hook_owned, payload);
            });

            Ok(())
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    brp_table.set("connect", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

async fn perform_connect(
    handle: &tauri::AppHandle,
    endpoint: String,
    timeout_ms: u64,
) -> serde_json::Value {
    let client = match BrpClient::new(endpoint.clone(), Duration::from_millis(timeout_ms)) {
        Ok(c) => c,
        Err(e) => return error_envelope("transport", &e.to_string(), None, None),
    };
    // Phase 1.2: hard-probe rpc.discover + soft-probe registry.schema so
    // the plugin can read `status.capabilities.{name_types,…}` immediately
    // after `arbor.brp.connect` resolves.
    let caps = match probe_capabilities(&client).await {
        Ok(caps) => caps,
        Err(e) => return error_from_brp(e),
    };
    let session = BrpSession::new(endpoint, client).with_capabilities(caps);
    let status = BrpStatus::from_session(Some(&session));
    let state = handle.state::<crate::AppState>();
    if let Ok(mut reg) = state.brp.lock() {
        reg.set(session);
    } else {
        return error_envelope("internal", "brp registry mutex poisoned", None, None);
    }
    serde_json::json!({ "ok": true, "result": status })
}

// ─── disconnect ──────────────────────────────────────────────────────────

fn install_disconnect(ctx: &ApiCtx, lua: &Lua, brp_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, _: ()| {
            let Some(ref h) = handle else {
                return Err(mlua::Error::RuntimeError(
                    "arbor.brp.disconnect: app handle unavailable".into(),
                ));
            };
            let state = h.state::<crate::AppState>();
            if let Ok(mut reg) = state.brp.lock() {
                reg.clear();
            }
            let status = BrpStatus::from_session(None);
            Ok(lua_ctx.to_value(&status).unwrap_or(mlua::Value::Nil))
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    brp_table.set("disconnect", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── status ──────────────────────────────────────────────────────────────

fn install_status(ctx: &ApiCtx, lua: &Lua, brp_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, _: ()| {
            let Some(ref h) = handle else {
                return Err(mlua::Error::RuntimeError(
                    "arbor.brp.status: app handle unavailable".into(),
                ));
            };
            let state = h.state::<crate::AppState>();
            let status = state
                .brp
                .lock()
                .map(|reg| BrpStatus::from_session(reg.session()))
                .unwrap_or_else(|_| BrpStatus::from_session(None));
            Ok(lua_ctx.to_value(&status).unwrap_or(mlua::Value::Nil))
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    brp_table.set("status", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── call ────────────────────────────────────────────────────────────────

fn install_call(
    ctx: &ApiCtx,
    lua: &Lua,
    brp_table: &Table,
    counter: Arc<AtomicU64>,
) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let handle = ctx.app_handle.clone();

    let fn_ = lua
        .create_function(move |lua_ctx, args: mlua::MultiValue| {
            let (method, params_value, callback) = parse_call_args(lua_ctx, args)?;

            let Some(ref h) = handle else {
                return Err(mlua::Error::RuntimeError(
                    "arbor.brp.call: app handle unavailable".into(),
                ));
            };

            // Snapshot the live session's client (Arc clone). If none,
            // error out immediately into the callback.
            let state = h.state::<crate::AppState>();
            let client = match state.brp.lock() {
                Ok(reg) => reg.session().map(|s| s.client.clone()),
                Err(_) => {
                    fire_error_inline(
                        lua_ctx,
                        &pname,
                        callback,
                        "internal",
                        "brp registry mutex poisoned",
                    )?;
                    return Ok(());
                }
            };
            let Some(client) = client else {
                fire_error_inline(
                    lua_ctx,
                    &pname,
                    callback,
                    "not_connected",
                    "BRP not connected — call arbor.brp.connect first",
                )?;
                return Ok(());
            };

            let hook_name = reserve_callback(lua_ctx, &pname, &counter, "call", callback)?;
            let h = h.clone();
            let pname_owned = pname.clone();
            let hook_owned = hook_name;

            tauri::async_runtime::spawn(async move {
                let payload = match client.call(&method, params_value).await {
                    Ok(value) => serde_json::json!({ "ok": true, "result": value }),
                    Err(e) => error_from_brp(e),
                };
                fire_callback(&h, &pname_owned, &hook_owned, payload);
            });

            Ok(())
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    brp_table.set("call", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── watch ───────────────────────────────────────────────────────────────

fn install_watch(
    ctx: &ApiCtx,
    lua: &Lua,
    brp_table: &Table,
    counter: Arc<AtomicU64>,
) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let handle = ctx.app_handle.clone();

    let fn_ = lua
        .create_function(move |lua_ctx, args: mlua::MultiValue| {
            let (method, params_value, callback) = parse_call_args(lua_ctx, args)?;

            let Some(ref h) = handle else {
                return Err(mlua::Error::RuntimeError(
                    "arbor.brp.watch: app handle unavailable".into(),
                ));
            };

            // Snapshot endpoint from the active session. If none, fire one
            // error/close pair into the callback and bail — same shape the
            // streamer would have emitted, so the plugin only needs one
            // failure-handling path.
            let state = h.state::<crate::AppState>();
            let endpoint = match state.brp.lock() {
                Ok(reg) => reg.session().map(|s| s.endpoint.clone()),
                Err(_) => {
                    fire_watch_error_inline(
                        lua_ctx,
                        &pname,
                        callback,
                        "internal",
                        "brp registry mutex poisoned",
                    )?;
                    return Ok(mlua::Value::Nil);
                }
            };
            let Some(endpoint) = endpoint else {
                fire_watch_error_inline(
                    lua_ctx,
                    &pname,
                    callback,
                    "not_connected",
                    "BRP not connected — call arbor.brp.connect first",
                )?;
                return Ok(mlua::Value::Nil);
            };

            // Reserve a long-lived hook entry — fired once per SSE event.
            // The hook stays registered until `unwatch` (or plugin reload)
            // tears it down via `remove_hook`.
            let hook_name = reserve_callback(lua_ctx, &pname, &counter, "watch", callback)?;
            let h = h.clone();
            let pname_owned = pname.clone();
            let method_owned = method.clone();
            let hook_owned = hook_name.clone();

            // Allocate the subscription id up-front so we can return it
            // immediately; the streaming task picks the same id when it
            // populates the registry entry on first poll.
            let sub_id = {
                let mut reg = state.brp.lock().map_err(|_| {
                    mlua::Error::RuntimeError("brp registry mutex poisoned".into())
                })?;
                reg.next_watch_id()
            };

            let h_for_task = h.clone();
            let pname_for_task = pname_owned.clone();
            let hook_for_task = hook_owned.clone();

            let join = tokio::spawn(async move {
                run_watch_stream(
                    endpoint,
                    method_owned,
                    params_value,
                    move |event| {
                        let payload = watch_event_to_payload(&event);
                        fire_callback(&h_for_task, &pname_for_task, &hook_for_task, payload);
                    },
                )
                .await;
                // Stream exited on its own (server closed / network drop /
                // RPC error tail) — drop the registry entry so it doesn't
                // leak. Manual unwatch removes the entry first, so this is
                // a no-op in that path.
                if let Ok(mut reg) = h.state::<crate::AppState>().brp.lock() {
                    if reg.take_watch(sub_id).is_some() {
                        // Also yank the hook so the closure is freed.
                        if let Ok(host) = h.state::<crate::AppState>().plugin_host.lock() {
                            let _ = host.remove_hook(&pname_owned, &hook_owned);
                        }
                    }
                }
            });

            // Park the AbortHandle so unwatch / disconnect can cancel us.
            if let Ok(mut reg) = state.brp.lock() {
                reg.insert_watch(WatchSub {
                    id: sub_id,
                    plugin: pname.clone(),
                    method,
                    hook_name: hook_name.clone(),
                    aborter: join.abort_handle(),
                });
            }

            Ok(mlua::Value::Integer(sub_id as mlua::Integer))
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    brp_table.set("watch", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── unwatch ─────────────────────────────────────────────────────────────

fn install_unwatch(ctx: &ApiCtx, lua: &Lua, brp_table: &Table) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let handle = ctx.app_handle.clone();

    let fn_ = lua
        .create_function(move |_lua_ctx, sub_id: mlua::Integer| {
            let Some(ref h) = handle else { return Ok(false); };
            if sub_id <= 0 { return Ok(false); }
            let id = sub_id as u64;
            let state = h.state::<crate::AppState>();
            let sub = match state.brp.lock() {
                Ok(mut reg) => reg.take_watch(id),
                Err(_) => return Ok(false),
            };
            let Some(sub) = sub else { return Ok(false); };
            // Defence in depth: a plugin shouldn't be able to drop another
            // plugin's subscription. We don't surface a hard error since
            // sub ids leak through globals anyway — silently no-op.
            if sub.plugin != pname {
                // Put it back; we shouldn't have removed it.
                if let Ok(mut reg) = state.brp.lock() {
                    reg.insert_watch(sub);
                }
                return Ok(false);
            }
            sub.aborter.abort();
            if let Ok(host) = state.plugin_host.lock() {
                let _ = host.remove_hook(&pname, &sub.hook_name);
            }
            Ok(true)
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    brp_table.set("unwatch", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn watch_event_to_payload(event: &WatchEvent) -> serde_json::Value {
    match event {
        WatchEvent::Open => serde_json::json!({ "ok": true, "event": "open" }),
        WatchEvent::Data(v) => serde_json::json!({ "ok": true, "event": "data", "result": v }),
        WatchEvent::Close => serde_json::json!({ "ok": true, "event": "close" }),
        WatchEvent::Error(msg) => serde_json::json!({
            "ok": false,
            "event": "error",
            "error": { "kind": "transport", "message": msg },
        }),
        WatchEvent::RpcError { code, message, data } => {
            let mut err = serde_json::json!({
                "kind": "rpc",
                "message": message,
                "code": code,
            });
            if let Some(d) = data {
                err["data"] = d.clone();
            }
            serde_json::json!({ "ok": false, "event": "error", "error": err })
        }
    }
}

fn fire_watch_error_inline(
    lua_ctx: &Lua,
    pname: &str,
    callback: mlua::Function,
    kind: &str,
    message: &str,
) -> mlua::Result<()> {
    // Watch-shaped error+close pair. We deliver both synchronously so the
    // plugin sees the same termination sequence whether the stream died at
    // setup or mid-flight.
    let err = serde_json::json!({
        "ok": false,
        "event": "error",
        "error": { "kind": kind, "message": message },
    });
    let close = serde_json::json!({ "ok": true, "event": "close" });
    let _ = pname;
    callback.call::<()>(lua_ctx.to_value(&err)?)?;
    callback.call::<()>(lua_ctx.to_value(&close)?)?;
    Ok(())
}

// ─── helpers ─────────────────────────────────────────────────────────────

fn parse_opts_and_callback(
    args: mlua::MultiValue,
) -> mlua::Result<(Option<mlua::Table>, mlua::Function)> {
    let mut iter = args.into_iter();
    let first = iter.next();
    let second = iter.next();
    match (first, second) {
        (Some(mlua::Value::Function(cb)), None) => Ok((None, cb)),
        (Some(mlua::Value::Table(t)), Some(mlua::Value::Function(cb))) => Ok((Some(t), cb)),
        _ => Err(mlua::Error::RuntimeError(
            "arbor.brp.connect: expected (callback) or (opts, callback)".into(),
        )),
    }
}

fn parse_call_args(
    lua_ctx: &Lua,
    args: mlua::MultiValue,
) -> mlua::Result<(String, Option<serde_json::Value>, mlua::Function)> {
    let mut iter = args.into_iter();
    let method_v = iter.next().ok_or_else(|| {
        mlua::Error::RuntimeError("arbor.brp.call: method required".into())
    })?;
    let method = match method_v {
        mlua::Value::String(s) => s
            .to_str()
            .map(|c| c.to_string())
            .map_err(|_| mlua::Error::RuntimeError("arbor.brp.call: method must be utf-8".into()))?,
        _ => {
            return Err(mlua::Error::RuntimeError(
                "arbor.brp.call: method must be a string".into(),
            ));
        }
    };
    let (params, callback) = match (iter.next(), iter.next()) {
        (Some(mlua::Value::Function(cb)), None) => (None, cb),
        (Some(value), Some(mlua::Value::Function(cb))) => {
            // Convert the Lua value (table / scalar) to JSON via serde.
            let json: serde_json::Value = lua_ctx.from_value(value).map_err(|e| {
                mlua::Error::RuntimeError(format!("arbor.brp.call: params conversion failed: {e}"))
            })?;
            (Some(json), cb)
        }
        _ => {
            return Err(mlua::Error::RuntimeError(
                "arbor.brp.call: expected (method, callback) or (method, params, callback)".into(),
            ));
        }
    };
    Ok((method, params, callback))
}

fn reserve_callback(
    lua_ctx: &Lua,
    pname: &str,
    counter: &AtomicU64,
    tag: &str,
    callback: mlua::Function,
) -> mlua::Result<String> {
    let n = counter.fetch_add(1, Ordering::Relaxed);
    let hook_name = format!("__brp_{tag}_{pname}_{n}__");
    let registry: Table = lua_ctx.globals().get("__arbor_hooks__")?;
    let list = lua_ctx.create_table()?;
    list.push(callback)?;
    registry.set(hook_name.clone(), list)?;
    Ok(hook_name)
}

fn fire_callback(
    handle: &tauri::AppHandle,
    pname: &str,
    hook_name: &str,
    payload: serde_json::Value,
) {
    let payload_str = serde_json::to_string(&payload).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"serialise failed\"}}".into()
    });
    let state = handle.state::<crate::AppState>();
    if let Ok(host) = state.plugin_host.lock() {
        let _ = host.fire_hook_on(pname, hook_name, &payload_str);
    };
}

fn fire_error_inline(
    lua_ctx: &Lua,
    pname: &str,
    callback: mlua::Function,
    kind: &str,
    message: &str,
) -> mlua::Result<()> {
    // For synchronous error paths (permission, not_connected, …) we don't
    // need the async hook dance — call the Lua function directly. Keeps the
    // error visible in the same Lua stack frame for traceback purposes.
    let payload = error_envelope(kind, message, None, None);
    let lua_value = lua_ctx.to_value(&payload)?;
    let _ = pname; // currently unused for inline path; kept for symmetry
    callback.call::<()>(lua_value)
}

fn permission_gate(pname: &str, net_perm: &[String], endpoint: &str) -> std::result::Result<(), String> {
    if net_perm.is_empty() {
        return Err(format!(
            "arbor.brp.connect: '{pname}' requires `network` permission. \
             Add to plugin.toml: network = [\"127.0.0.1\"] (or [\"*\"])."
        ));
    }
    let host = endpoint
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(endpoint)
        .split(|c: char| c == '/' || c == ':' || c == '?' || c == '#')
        .next()
        .unwrap_or("")
        .to_string();
    if host.is_empty() {
        return Err(format!(
            "arbor.brp.connect: cannot parse host from endpoint '{endpoint}'"
        ));
    }
    let allowed = net_perm
        .iter()
        .any(|h| h == "*" || h == &host || host.ends_with(&format!(".{h}")));
    if !allowed {
        return Err(format!(
            "arbor.brp.connect: host '{host}' not in plugin's network allowlist {net_perm:?}"
        ));
    }
    Ok(())
}

fn error_envelope(
    kind: &str,
    message: &str,
    code: Option<i64>,
    data: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut err = serde_json::json!({ "kind": kind, "message": message });
    if let Some(c) = code {
        err["code"] = serde_json::json!(c);
    }
    if let Some(d) = data {
        err["data"] = d;
    }
    serde_json::json!({ "ok": false, "error": err })
}

fn error_from_brp(e: crate::brp::BrpError) -> serde_json::Value {
    use crate::brp::BrpError;
    match e {
        BrpError::Transport(m) => error_envelope("transport", &m, None, None),
        BrpError::Status { status, body } => error_envelope(
            "status",
            &format!("HTTP {status}: {body}"),
            Some(status as i64),
            None,
        ),
        BrpError::InvalidResponse(m) => error_envelope("invalid_response", &m, None, None),
        BrpError::Rpc { code, message, data } => error_envelope("rpc", &message, Some(code), data),
    }
}
