//! `arbor.service` — cross-plugin RPC (inter-VM dispatch).
//!
//! Providers expose named functions via `arbor.service.export`; consumers
//! invoke them asynchronously with `arbor.service.call(qualified, args, cb)`.
//! Arguments and return values travel as JSON. The callback receives
//! `(ok: boolean, result_or_error)` — on failure the second argument is a
//! typed error table `{ kind = <string>, message = <string> }`:
//!   not_found | plugin_disabled | handler_error
//!
//! Permissions:
//!   service_export = true  -> .export / .unexport / .list_own
//!   service_call   = true  -> .call / .list
//!
//! Nothing in this module takes the `PluginHost` mutex on the calling thread. `.call`
//! dispatches on a background thread; `.export` / `.unexport` / `.list_own` touch only the
//! plugin's own VM; `.list` reads the shared `ServiceIndex`. That is the whole point: the
//! host fires hooks while holding its mutex, so a synchronous lock here deadlocks the
//! backend the first time a plugin calls it from `arbor:plugin_load` — which `.list` did,
//! under a header that already promised it did not.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    if !(ctx.service_export || ctx.service_call) {
        return Ok(());
    }

    // Bootstrap per-plugin globals the Lua side relies on.
    lua.load(
        "__arbor_services__ = __arbor_services__ or {}\n\
         __arbor_service_callbacks__ = __arbor_service_callbacks__ or {}"
    ).exec().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let svc_table = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    if ctx.service_export {
        install_export(ctx, lua, &svc_table)?;
    }
    if ctx.service_call {
        install_call(ctx, lua, &svc_table)?;
        install_list(ctx, lua, &svc_table)?;
    }

    arbor.set("service", svc_table).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_export(ctx: &ApiCtx, lua: &Lua, svc_table: &Table) -> Result<()> {
    // The Lua table stays the source of truth for *dispatch* — `invoke_service` calls the
    // function out of it. The index beside it is the source of truth for *discovery*, which
    // is the half a reader needs without a Lua VM in hand. Both are written here so they
    // cannot drift: an export that reached one and not the other would be either an
    // uncallable listing or an invisible service.
    let plugin = ctx.plugin_name.clone();
    let index  = ctx.services.clone();

    // export(method, fn)
    let (p, idx) = (plugin.clone(), index.clone());
    let fn_ = lua.create_function(move |lua_ctx, (method, func): (String, mlua::Function)| {
        let reg: Table = lua_ctx.globals().get("__arbor_services__")?;
        reg.set(method.clone(), func)?;
        idx.export(&p, &method);
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    svc_table.set("export", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // unexport(method)
    let (p, idx) = (plugin.clone(), index.clone());
    let fn_ = lua.create_function(move |lua_ctx, method: String| {
        let reg: Table = lua_ctx.globals().get("__arbor_services__")?;
        reg.set(method.clone(), mlua::Value::Nil)?;
        idx.unexport(&p, &method);
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    svc_table.set("unexport", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // list_own() -> string[]
    let fn_ = lua.create_function(|lua_ctx, _: ()| {
        let reg: Table = lua_ctx.globals().get("__arbor_services__")?;
        let out = lua_ctx.create_table()?;
        for (k, _) in reg.pairs::<String, mlua::Function>().flatten() {
            out.push(k)?;
        }
        Ok(out)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    svc_table.set("list_own", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_call(ctx: &ApiCtx, lua: &Lua, svc_table: &Table) -> Result<()> {
    let counter: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let host   = ctx.host_weak.clone();
    let caller = ctx.plugin_name.clone();
    let counter_c = counter.clone();
    let fn_ = lua.create_function(
        move |lua_ctx, (qualified, args, cb): (String, Option<mlua::Value>, Option<mlua::Function>)| {
            let (target, method) = match qualified.find('.') {
                Some(i) => (qualified[..i].to_string(), qualified[i+1..].to_string()),
                None => return Err(mlua::Error::RuntimeError(format!(
                    "arbor.service.call: expected 'plugin.method', got '{qualified}'"
                ))),
            };

            let args_json: serde_json::Value = match args {
                None | Some(mlua::Value::Nil) => serde_json::Value::Null,
                Some(v) => lua_ctx.from_value(v)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
            };

            let id = counter_c.fetch_add(1, Ordering::Relaxed);
            let call_id = format!("svc-{id}");

            if let Some(c) = cb {
                let cbs: Table = lua_ctx.globals().get("__arbor_service_callbacks__")?;
                cbs.set(call_id.clone(), c)?;
            }

            let caller_p  = caller.clone();
            let target_p  = target;
            let method_p  = method;
            let host_c    = host.clone();
            let call_id_c = call_id.clone();
            std::thread::spawn(move || {
                if let Some(arc) = host_c.and_then(|w| w.upgrade()) {
                    if let Ok(host) = arc.lock() {
                        let (ok, payload) = match host.invoke_service(&target_p, &method_p, &args_json) {
                            Ok(v) => (true, v),
                            Err(e) => (false, serde_json::json!({
                                "kind":    e.kind(),
                                "message": e.message(),
                            })),
                        };
                        host.deliver_service_response(&caller_p, &call_id_c, ok, &payload);
                    }
                }
            });

            Ok(())
        },
    ).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    svc_table.set("call", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(ctx: &ApiCtx, lua: &Lua, svc_table: &Table) -> Result<()> {
    // Answered from the index, NOT by locking the host — see `ServiceIndex`. This used to
    // reflect over every plugin's Lua globals under the host mutex, which deadlocked the
    // backend outright when called from a hook (the host fires hooks while holding it) and
    // paid a full Lua table walk per plugin per call even when it did not.
    let index    = ctx.services.clone();
    let activity = ctx.activity.clone();
    let fn_ = lua.create_function(move |lua_ctx, _: ()| {
        let out = lua_ctx.create_table()?;
        for s in index.qualified(&activity) { out.push(s)?; }
        Ok(out)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    svc_table.set("list", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
