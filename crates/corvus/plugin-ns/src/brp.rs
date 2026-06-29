//! `arbor.brp.*` — Bevy Remote Protocol surface, ported to run inside any host
//! through an [`NsHost`].
//!
//! Lua-visible surface mirrors the shell's `ns_shell/brp.rs`: same namespace
//! (`arbor.brp`), same function names (`connect` / `disconnect` / `status` /
//! `call` / `watch` / `unwatch`), same argument shapes, same single-shot /
//! watch callback envelopes, same permission-gate error strings.
//!
//! ## PROXY namespace — and the callback-delivery gap
//!
//! The `BrpRegistry` (live HTTP client + SSE subscriptions) is tied to the
//! shell's `AppState` + plugin-host (the SSE streamer fires Lua callbacks back
//! through the shell's hook registry). It **stays shell-side**: every op here is
//! a reverse-channel round-trip — the `CorvusNsHost` impl calls
//! `host_call("__brp_<op>", …)` and the matching handler in
//! `src-tauri/src/ipc/mod.rs` reads/mutates the real registry exactly as
//! `ns_shell/brp.rs` did.
//!
//! `connect` / `disconnect` / `status` / `call` work end to end: `host_call`
//! **blocks** on the shell's reply, so we deliver the result by invoking the Lua
//! callback **directly, synchronously, in this VM** (the same inline-callback
//! path the shell already used for its `permission` / `not_connected` errors).
//! The original shell version spawned the HTTP work on a Tauri task and fired
//! the callback through the hook registry; here the round-trip already blocks,
//! so a direct call is both simpler and faithful — the plugin still sees exactly
//! one `{ ok = … }` envelope.
//!
//! **`watch` / `unwatch` deliver end to end.** `watch` registers an SSE
//! subscription on the shell and returns the real `sub_id`. The stream's `open` /
//! `data` / `close` / `error` events fire on the **shell** process, but each is
//! pushed back into this VM through the inverse `invoke_plugin_callback` RPC:
//! `watch` mints a BE callback id, parks the Lua closure under it in this VM's
//! `__arbor_hooks__` registry (the same mechanism `arbor.job.spawn` uses for
//! `on_done`), and forwards `{ plugin, callback_id }` to the shell so it can route
//! every `watch_event_to_payload` envelope back here. `unwatch` (and stream-end)
//! drives `remove_plugin_callback` so the parked closure is dropped.

use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    ApiCtx, LuaNamespaceInstaller, PluginCoreError, PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// Monotonic counter for the synthetic BE-side `watch` callback ids. Each
/// `arbor.brp.watch` mints one — the parked Lua closure lives under
/// `__arbor_hooks__[<callback_id>]` and the shell stores the same id on its
/// `WatchSub` so it can route every SSE event back here (and drop the closure on
/// teardown). Process-wide is fine: the id only has to be unique within this VM's
/// hook registry.
static WATCH_CALLBACK_SEQ: AtomicU64 = AtomicU64::new(0);

/// `arbor.brp.*` installer. Holds the host handle the closures call through.
pub struct BrpInstaller {
    host: NsHostHandle,
}

impl BrpInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for BrpInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let t = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_connect(self.host.clone(), ctx, lua, &t)?;
        install_disconnect(self.host.clone(), lua, &t)?;
        install_status(self.host.clone(), lua, &t)?;
        install_call(self.host.clone(), lua, &t)?;
        install_watch(self.host.clone(), ctx, lua, &t)?;
        install_unwatch(self.host.clone(), lua, &t)?;

        arbor
            .set("brp", t)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:15702";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;

// ─── connect ─────────────────────────────────────────────────────────────────

fn install_connect(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    // Snapshot the network allowlist at install time, exactly as the shell's
    // `ns_shell/brp.rs` did (`ctx.network_perm.clone()`).
    let pname = ctx.plugin_name.clone();
    let net_perm = ctx.network_perm.clone();

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

            // Permission gate runs installer-side (the allowlist is a load-time
            // snapshot, not shell state) — same RuntimeError-free inline-error
            // path the shell used.
            if let Err(msg) = permission_gate(&pname, &net_perm, &endpoint) {
                let payload = error_envelope("permission", &msg);
                return deliver_inline(lua_ctx, &callback, payload);
            }

            // `host_call("__brp_connect", …)` blocks on the shell's reply, which
            // is the full `{ ok = … }` envelope (ok|err) the shell's
            // `perform_connect` produced. Deliver it directly to the callback.
            let payload = host
                .brp_connect(&endpoint, timeout_ms)
                .unwrap_or_else(|e| error_envelope("internal", &e));
            deliver_inline(lua_ctx, &callback, payload)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("connect", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── disconnect ────────────────────────────────────────────────────────────────

fn install_disconnect(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, _: ()| {
            // Returns the cleared `BrpStatus` as a Lua table (the shell returned
            // `BrpStatus::from_session(None)`). On a host-call failure, surface
            // the disconnected status anyway (the shell never errored here).
            let status = host
                .brp_disconnect()
                .unwrap_or_else(|_| serde_json::json!({ "connected": false }));
            Ok(lua_ctx.to_value(&status).unwrap_or(mlua::Value::Nil))
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("disconnect", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── status ────────────────────────────────────────────────────────────────────

fn install_status(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, _: ()| {
            let status = host
                .brp_status()
                .unwrap_or_else(|_| serde_json::json!({ "connected": false }));
            Ok(lua_ctx.to_value(&status).unwrap_or(mlua::Value::Nil))
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("status", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── call ──────────────────────────────────────────────────────────────────────

fn install_call(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, args: mlua::MultiValue| {
            let (method, params_value, callback) = parse_call_args(lua_ctx, args)?;
            // `host_call("__brp_call", …)` blocks; the reply is the full
            // `{ ok = … }` envelope (ok | not_connected | rpc | transport | …).
            let payload = host
                .brp_call(&method, params_value)
                .unwrap_or_else(|e| error_envelope("internal", &e));
            deliver_inline(lua_ctx, &callback, payload)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("call", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── watch ─────────────────────────────────────────────────────────────────────

fn install_watch(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, args: mlua::MultiValue| {
            let (method, params_value, callback) = parse_call_args(lua_ctx, args)?;

            // Mint a BE-side callback id and PARK the Lua closure under it in this
            // VM's `__arbor_hooks__` registry — same mechanism `arbor.job.spawn`
            // uses for its `on_done` closure (see `job.rs::install_spawn`). The
            // shell stores this id on its `WatchSub` and routes every SSE event
            // back to `invoke_plugin_callback{ plugin_name, callback_id }`, which
            // fires the parked closure here. Teardown (`unwatch` / stream-end)
            // drives `remove_plugin_callback` so the closure is dropped.
            let n = WATCH_CALLBACK_SEQ.fetch_add(1, Ordering::Relaxed);
            let callback_id = format!("__brp_watch_{n}__");
            {
                let registry: Table = lua_ctx.globals().get("__arbor_hooks__")?;
                let list = lua_ctx.create_table()?;
                list.push(callback)?;
                registry.set(callback_id.clone(), list)?;
            }

            // Forward the real watch params alongside the routing metadata the
            // shell needs (real plugin name + callback id). The shell's
            // `__brp_watch` arm pops `__watch_meta` off and uses the inner
            // `params` as the actual BRP request body, so the metadata never
            // leaks into the SSE call.
            let wrapped = serde_json::json!({
                "__watch_meta": { "plugin": pname.clone(), "callback_id": callback_id.clone() },
                "params": params_value,
            });

            match host.brp_watch(&method, Some(wrapped)) {
                Ok(sub_id) => Ok(mlua::Value::Integer(sub_id as mlua::Integer)),
                // A failed registration is reported as `0` (no valid sub id) —
                // the shell's `watch` returned nil on the not-connected path; in
                // this best-effort port a falsy `0` keeps the Lua side simple.
                // Drop the now-orphaned parked closure so the registry doesn't
                // leak it (no SSE events will ever route to it).
                Err(_) => {
                    if let Ok(registry) = lua_ctx.globals().get::<Table>("__arbor_hooks__") {
                        let _ = registry.set(callback_id.clone(), mlua::Value::Nil);
                    }
                    Ok(mlua::Value::Integer(0))
                }
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("watch", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── unwatch ───────────────────────────────────────────────────────────────────

fn install_unwatch(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |_lua_ctx, sub_id: mlua::Integer| {
            if sub_id <= 0 {
                return Ok(false);
            }
            // Proxy the teardown so the shell-side SSE stream stops even though
            // we never delivered its events.
            Ok(host.brp_unwatch(sub_id as u64).unwrap_or(false))
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("unwatch", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── helpers ───────────────────────────────────────────────────────────────────

/// Invoke the single-shot Lua callback directly with the envelope, in this VM.
/// The host round-trip already blocked on the result, so there is nothing async
/// to dispatch — this keeps the envelope visible in the same Lua stack frame
/// (matching the shell's `fire_error_inline` path).
fn deliver_inline(
    lua_ctx: &Lua,
    callback: &mlua::Function,
    payload: serde_json::Value,
) -> mlua::Result<()> {
    let lua_value = lua_ctx.to_value(&payload)?;
    callback.call::<()>(lua_value)
}

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
    let method_v = iter
        .next()
        .ok_or_else(|| mlua::Error::RuntimeError("arbor.brp.call: method required".into()))?;
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

/// Single-shot error envelope `{ ok = false, error = { kind, message } }` —
/// the shell's `error_envelope` shape without the optional `code`/`data` (those
/// only arrive from the shell inside the proxied `host.brp_*` reply).
fn error_envelope(kind: &str, message: &str) -> serde_json::Value {
    serde_json::json!({ "ok": false, "error": { "kind": kind, "message": message } })
}

/// Host-name allowlist check, byte-for-byte the shell's `permission_gate`.
fn permission_gate(
    pname: &str,
    net_perm: &[String],
    endpoint: &str,
) -> std::result::Result<(), String> {
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
        .split(['/', ':', '?', '#'])
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
