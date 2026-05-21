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
//! Calls are always dispatched on a background thread so the caller never
//! blocks and we can't deadlock on the non-reentrant PluginHost mutex.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    if !(ctx.service_export || ctx.service_call) {
        return Ok(());
    }

    // Bootstrap per-plugin globals the Lua side relies on.
    lua.load(
        "__arbor_services__ = __arbor_services__ or {}\n\
         __arbor_service_callbacks__ = __arbor_service_callbacks__ or {}"
    ).exec().map_err(|e| AppError::Plugin(e.to_string()))?;

    let svc_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    if ctx.service_export {
        install_export(lua, &svc_table)?;
    }
    if ctx.service_call {
        install_call(ctx, lua, &svc_table)?;
        install_list(ctx, lua, &svc_table)?;
    }

    arbor.set("service", svc_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_export(lua: &Lua, svc_table: &Table) -> Result<()> {
    // export(method, fn)
    let fn_ = lua.create_function(|lua_ctx, (method, func): (String, mlua::Function)| {
        let reg: Table = lua_ctx.globals().get("__arbor_services__")?;
        reg.set(method, func)?;
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    svc_table.set("export", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;

    // unexport(method)
    let fn_ = lua.create_function(|lua_ctx, method: String| {
        let reg: Table = lua_ctx.globals().get("__arbor_services__")?;
        reg.set(method, mlua::Value::Nil)?;
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    svc_table.set("unexport", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;

    // list_own() -> string[]
    let fn_ = lua.create_function(|lua_ctx, _: ()| {
        let reg: Table = lua_ctx.globals().get("__arbor_services__")?;
        let out = lua_ctx.create_table()?;
        for pair in reg.pairs::<String, mlua::Function>() {
            if let Ok((k, _)) = pair { out.push(k)?; }
        }
        Ok(out)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    svc_table.set("list_own", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_call(ctx: &ApiCtx, lua: &Lua, svc_table: &Table) -> Result<()> {
    let counter: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let handle = ctx.app_handle.clone();
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
            let handle_c  = handle.clone();
            let call_id_c = call_id.clone();
            std::thread::spawn(move || {
                if let Some(ref h) = handle_c {
                    let state = h.state::<crate::AppState>();
                    if let Ok(host) = state.plugin_host.lock() {
                        let (ok, payload) = match host.invoke_service(&target_p, &method_p, &args_json) {
                            Ok(v) => (true, v),
                            Err(e) => (false, serde_json::json!({
                                "kind":    e.kind(),
                                "message": e.message(),
                            })),
                        };
                        host.deliver_service_response(&caller_p, &call_id_c, ok, &payload);
                    };
                }
            });

            Ok(())
        },
    ).map_err(|e| AppError::Plugin(e.to_string()))?;
    svc_table.set("call", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(ctx: &ApiCtx, lua: &Lua, svc_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, _: ()| {
        let out = lua_ctx.create_table()?;
        if let Some(ref h) = handle {
            let state = h.state::<crate::AppState>();
            if let Ok(host) = state.plugin_host.lock() {
                for s in host.list_all_services() { out.push(s)?; }
            };
        }
        Ok(out)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    svc_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
