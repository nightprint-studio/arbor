//! `arbor.cloud.*` — Lua surface for the cloud-storage plugin.
//!
//! No permission gate yet (the plugin is single-purpose and any plugin
//! adopting these APIs is opting into the same trust model as the
//! cloud-storage plugin itself). When the WASM runtime lands this whole
//! file is deleted alongside `crate::cloud`.
//!
//! All operations follow the `(value, nil) | (nil, err_msg)` convention.
//! Connection details (`conn`) and operation args travel as Lua tables;
//! the host deserialises them via serde so the Lua surface stays close
//! to the Rust types.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};
use crate::cloud::types::CloudConnection;

pub(crate) fn install(_ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_secrets(lua, &table)?;
    install_test_connection(lua, &table)?;
    install_test_connection_async(_ctx, lua, &table)?;
    install_list(lua, &table)?;
    install_list_stream(_ctx, lua, &table)?;
    install_search_stream(_ctx, lua, &table)?;
    install_cancel(_ctx, lua, &table)?;
    install_is_cancelled(_ctx, lua, &table)?;
    install_stat(lua, &table)?;
    install_delete(lua, &table)?;
    install_copy(lua, &table)?;
    install_download(_ctx, lua, &table)?;
    install_upload(_ctx, lua, &table)?;
    install_sync(_ctx, lua, &table)?;
    install_oauth_start(_ctx, lua, &table)?;
    install_download_many(_ctx, lua, &table)?;
    install_concat_files(lua, &table)?;
    install_report_progress(_ctx, lua, &table)?;
    install_report_done(_ctx, lua, &table)?;
    install_pick_chunk_order(_ctx, lua, &table)?;

    arbor.set("cloud", table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── async dispatch helper ──────────────────────────────────────────────────

/// Run an async future from inside a Lua closure.
///
/// Lua plugin callbacks are dispatched on the plugin-host thread, which has
/// no tokio runtime attached — `Handle::current()` would panic. We delegate
/// to `tauri::async_runtime::block_on`, which always uses the long-lived
/// runtime Tauri created at startup (the same one `#[tauri::command]
/// async fn` uses). That matters specifically for the transfer ops, which
/// `tokio::spawn` detached tasks internally — those tasks need a runtime
/// that outlives this call. A throwaway `Runtime::new()` would be dropped
/// immediately and the spawned tasks would die before doing any work.
macro_rules! block_on {
    ($fut:expr) => {{
        tauri::async_runtime::block_on($fut)
    }};
}

// ── Conversion helpers ─────────────────────────────────────────────────────

fn conn_from_table(lua_ctx: &Lua, t: &Table) -> std::result::Result<CloudConnection, String> {
    let v: serde_json::Value = lua_ctx.from_value(mlua::Value::Table(t.clone()))
        .map_err(|e| format!("decode conn: {e}"))?;
    serde_json::from_value(v).map_err(|e| format!("invalid conn: {e}"))
}

fn opt_str(t: &Table, key: &str) -> Option<String> {
    t.get::<Option<String>>(key).ok().flatten()
}

fn opt_bool(t: &Table, key: &str) -> Option<bool> {
    t.get::<Option<bool>>(key).ok().flatten()
}

fn opt_usize(t: &Table, key: &str) -> Option<usize> {
    t.get::<Option<i64>>(key).ok().flatten().map(|n| n.max(0) as usize)
}

fn req_str(t: &Table, key: &str, op: &str) -> std::result::Result<String, String> {
    opt_str(t, key).ok_or_else(|| format!("{op}: missing required field `{key}`"))
}

fn require_conn(lua_ctx: &Lua, opts: &Table, op: &str) -> std::result::Result<CloudConnection, String> {
    let conn_t: Table = opts.get("conn")
        .map_err(|_| format!("{op}: missing required `conn` table"))?;
    conn_from_table(lua_ctx, &conn_t)
}

// ── secrets ────────────────────────────────────────────────────────────────

fn install_secrets(lua: &Lua, table: &Table) -> Result<()> {
    // arbor.cloud.secret_set(secret_ref, value)
    let f = lua.create_function(|_lua_ctx, (r, v): (String, String)| -> LuaTuple {
        match crate::cloud::secrets::set(&r, &v) {
            Ok(()) => ok2(_lua_ctx, mlua::Value::Boolean(true)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("secret_set", f).map_err(|e| AppError::Plugin(e.to_string()))?;

    let f = lua.create_function(|_lua_ctx, r: String| -> LuaTuple {
        match crate::cloud::secrets::exists(&r) {
            Ok(b)  => ok2(_lua_ctx, mlua::Value::Boolean(b)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("secret_exists", f).map_err(|e| AppError::Plugin(e.to_string()))?;

    let f = lua.create_function(|_lua_ctx, r: String| -> LuaTuple {
        match crate::cloud::secrets::delete(&r) {
            Ok(()) => ok2(_lua_ctx, mlua::Value::Boolean(true)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("secret_delete", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── test_connection ────────────────────────────────────────────────────────

fn install_test_connection(lua: &Lua, table: &Table) -> Result<()> {
    let f = lua.create_function(|_lua_ctx, opts: Table| -> LuaTuple {
        let conn = match require_conn(_lua_ctx, &opts, "arbor.cloud.test_connection") {
            Ok(c)  => c,
            Err(e) => return err2(_lua_ctx, e),
        };
        let bucket = opt_str(&opts, "bucket");
        let res = block_on!(crate::cloud::ops::test_connection(&conn, bucket.as_deref()));
        match res {
            Ok(r)  => {
                let json = serde_json::to_value(&r).unwrap_or_default();
                ok2(_lua_ctx, _lua_ctx.to_value(&json).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?)
            }
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("test_connection", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// Non-blocking variant: returns immediately and delivers the result via
/// `arbor.events.on(<on_done>, fn)`. Required from form-handler contexts —
/// the synchronous version holds the plugin-host thread for the full HTTP
/// round-trip, which made the rest of the app appear to freeze.
///
/// Lua signature:
///   arbor.cloud.test_connection_async({
///     conn       = { … },         -- same envelope as test_connection
///     bucket     = "name",        -- optional
///     on_done    = "event:name",  -- event name the result is fired under
///     request_id = "form-1234",   -- echoed back on the result so multiple
///                                    in-flight tests don't clobber each other
///   })
///
/// Done-event payload shape:
///   { request_id = "...", ok = true,  reply = { … } }
///   { request_id = "...", ok = false, error = "..." }
fn install_test_connection_async(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.test_connection_async";
        let Some(ref h) = handle else { return err2(lua_ctx, format!("{op}: app handle unavailable")); };
        let conn = match require_conn(lua_ctx, &opts, op) {
            Ok(c)  => c,
            Err(e) => return err2(lua_ctx, e),
        };
        let bucket     = opt_str(&opts, "bucket");
        let on_done    = match req_str(&opts, "on_done", op) { Ok(s) => s, Err(e) => return err2(lua_ctx, e) };
        let request_id = opt_str(&opts, "request_id").unwrap_or_default();

        let app = h.clone();
        tauri::async_runtime::spawn(async move {
            let res = crate::cloud::ops::test_connection(&conn, bucket.as_deref()).await;
            // Build payload as a JSON string — fire_hook decodes it back into
            // a Lua table for each subscriber. Done off the tokio worker so we
            // don't block it on the plugin host's mutex.
            let payload = match res {
                Ok(r) => serde_json::json!({
                    "request_id": request_id,
                    "ok":         true,
                    "reply":      r,
                }),
                Err(e) => serde_json::json!({
                    "request_id": request_id,
                    "ok":         false,
                    "error":      e.to_string(),
                }),
            };
            let payload_str = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
            std::thread::spawn(move || {
                let state = app.state::<crate::AppState>();
                if let Ok(host) = state.plugin_host.lock() {
                    let _ = host.fire_hook(&on_done, &payload_str);
                };
            });
        });
        ok2(lua_ctx, mlua::Value::Boolean(true))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("test_connection_async", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── list / stat / delete / copy ────────────────────────────────────────────

fn install_list(lua: &Lua, table: &Table) -> Result<()> {
    let f = lua.create_function(|_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.list";
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c,
            Err(e) => return err2(_lua_ctx, e),
        };
        let bucket = match req_str(&opts, "bucket", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let prefix = opt_str(&opts, "prefix").unwrap_or_default();
        let limit  = opt_usize(&opts, "limit");
        let res = block_on!(crate::cloud::ops::list(&conn, &bucket, &prefix, limit));
        match res {
            Ok(page) => {
                let json = serde_json::to_value(&page).unwrap_or_default();
                ok2(_lua_ctx, _lua_ctx.to_value(&json).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?)
            }
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("list", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list_stream(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.list_stream";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket    = match req_str(&opts, "bucket",    op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let stream_id = match req_str(&opts, "stream_id", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let prefix    = opt_str(&opts, "prefix");
        let cap       = opt_usize(&opts, "cap");

        // Register the cancel flag eagerly so subsequent `arbor.cloud.cancel`
        // calls can find it even before the spawned task has its first tick.
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let state = h.state::<crate::AppState>();
            if let Ok(mut map) = state.cloud_cancellations.lock() {
                map.insert(stream_id.clone(), cancel.clone());
            };
        }

        let app    = h.clone();
        let sid    = stream_id.clone();
        let bk     = bucket.clone();
        let pref   = prefix.unwrap_or_default();
        tauri::async_runtime::spawn(async move {
            let _ = crate::cloud::ops::list_stream(app.clone(), conn, bk, pref, sid.clone(), cap, cancel).await;
            let st = app.state::<crate::AppState>();
            if let Ok(mut map) = st.cloud_cancellations.lock() {
                map.remove(&sid);
            };
        });
        ok2(_lua_ctx, mlua::Value::String(_lua_ctx.create_string(&stream_id)?))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("list_stream", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_search_stream(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.search_stream";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket    = match req_str(&opts, "bucket",    op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let stream_id = match req_str(&opts, "stream_id", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let pattern   = match req_str(&opts, "pattern",   op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let root_prefix = opt_str(&opts, "root_prefix").unwrap_or_default();

        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let state = h.state::<crate::AppState>();
            if let Ok(mut map) = state.cloud_cancellations.lock() {
                map.insert(stream_id.clone(), cancel.clone());
            };
        }

        let app = h.clone();
        let sid = stream_id.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::cloud::ops::search_stream(
                app.clone(), conn, bucket, root_prefix, pattern, sid.clone(), cancel,
            ).await;
            let st = app.state::<crate::AppState>();
            if let Ok(mut map) = st.cloud_cancellations.lock() {
                map.remove(&sid);
            };
        });
        ok2(_lua_ctx, mlua::Value::String(_lua_ctx.create_string(&stream_id)?))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("search_stream", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_cancel(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, stream_id: String| -> LuaTuple {
        let Some(ref h) = handle else { return err2(_lua_ctx, "arbor.cloud.cancel: app handle unavailable"); };
        let state = h.state::<crate::AppState>();
        if let Ok(map) = state.cloud_cancellations.lock() {
            if let Some(flag) = map.get(&stream_id) {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        };
        ok2(_lua_ctx, mlua::Value::Boolean(true))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("cancel", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_is_cancelled(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, stream_id: String| -> LuaTuple {
        let Some(ref h) = handle else { return err2(_lua_ctx, "arbor.cloud.is_cancelled: app handle unavailable"); };
        let state = h.state::<crate::AppState>();
        let cancelled = if let Ok(map) = state.cloud_cancellations.lock() {
            map.get(&stream_id)
                .map(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(false)
        } else { false };
        ok2(_lua_ctx, mlua::Value::Boolean(cancelled))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("is_cancelled", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_stat(lua: &Lua, table: &Table) -> Result<()> {
    let f = lua.create_function(|_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.stat";
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket = match req_str(&opts, "bucket", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let path   = match req_str(&opts, "path",   op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let res = block_on!(crate::cloud::ops::stat(&conn, &bucket, &path));
        match res {
            Ok(o) => {
                let json = serde_json::to_value(&o).unwrap_or_default();
                ok2(_lua_ctx, _lua_ctx.to_value(&json).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?)
            }
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("stat", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_delete(lua: &Lua, table: &Table) -> Result<()> {
    let f = lua.create_function(|_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.delete";
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket    = match req_str(&opts, "bucket", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let path      = match req_str(&opts, "path",   op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let recursive = opt_bool(&opts, "recursive").unwrap_or(false);
        let res = block_on!(crate::cloud::ops::delete(&conn, &bucket, &path, recursive));
        match res {
            Ok(()) => ok2(_lua_ctx, mlua::Value::Boolean(true)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("delete", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_copy(lua: &Lua, table: &Table) -> Result<()> {
    let f = lua.create_function(|_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.copy";
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket = match req_str(&opts, "bucket", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let src    = match req_str(&opts, "src",    op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let dst    = match req_str(&opts, "dst",    op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let res = block_on!(crate::cloud::ops::copy(&conn, &bucket, &src, &dst));
        match res {
            Ok(()) => ok2(_lua_ctx, mlua::Value::Boolean(true)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("copy", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── transfers (return job_id) ──────────────────────────────────────────────

fn install_download(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.download";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket = match req_str(&opts, "bucket", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let path   = match req_str(&opts, "path",   op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let local  = match req_str(&opts, "local",  op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let res = block_on!(crate::cloud::transfer::download(
            h.clone(), conn, bucket, path, std::path::PathBuf::from(local),
        ));
        match res {
            Ok(id) => ok2(_lua_ctx, mlua::Value::String(_lua_ctx.create_string(&id)?)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("download", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_upload(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.upload";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket    = match req_str(&opts, "bucket", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let path      = match req_str(&opts, "path",   op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let local     = match req_str(&opts, "local",  op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let overwrite = opt_bool(&opts, "overwrite").unwrap_or(false);
        let res = block_on!(crate::cloud::transfer::upload(
            h.clone(), conn, bucket, path, std::path::PathBuf::from(local), overwrite,
        ));
        match res {
            Ok(id) => ok2(_lua_ctx, mlua::Value::String(_lua_ctx.create_string(&id)?)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("upload", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_sync(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.sync";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket        = match req_str(&opts, "bucket",        op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let remote_prefix = match req_str(&opts, "remote_prefix", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let local         = match req_str(&opts, "local",         op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let direction     = match req_str(&opts, "direction",     op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let delete        = opt_bool(&opts, "delete").unwrap_or(false);
        let dir = match direction.as_str() {
            "up"   => crate::cloud::transfer::SyncDir::Up,
            "down" => crate::cloud::transfer::SyncDir::Down,
            other  => return err2(_lua_ctx, format!("{op}: direction must be \"up\" or \"down\", got {other:?}")),
        };
        let res = block_on!(crate::cloud::transfer::sync(
            h.clone(), conn, bucket, remote_prefix,
            std::path::PathBuf::from(local), dir, delete,
        ));
        match res {
            Ok(id) => ok2(_lua_ctx, mlua::Value::String(_lua_ctx.create_string(&id)?)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("sync", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── download_many ──────────────────────────────────────────────────────────

fn install_download_many(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.download_many";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let conn = match require_conn(_lua_ctx, &opts, op) {
            Ok(c)  => c, Err(e) => return err2(_lua_ctx, e),
        };
        let bucket    = match req_str(&opts, "bucket",    op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let local_dir = match req_str(&opts, "local_dir", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let stream_id = match req_str(&opts, "stream_id", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let parallel  = opts.get::<Option<usize>>("parallel").ok().flatten();
        let op_label  = opt_str(&opts, "op_label");

        // `paths` is a Lua array of strings.
        let paths_t: Table = match opts.get("paths") {
            Ok(t)  => t,
            Err(_) => return err2(_lua_ctx, format!("{op}: missing required `paths` array")),
        };
        let mut paths: Vec<String> = Vec::new();
        for v in paths_t.sequence_values::<String>().flatten() {
            paths.push(v);
        }
        if paths.is_empty() {
            return err2(_lua_ctx, format!("{op}: `paths` must contain at least one entry"));
        }

        // Optional: extra OperationsOverlay steps appended after the per-file
        // download steps. Used by the chunk-merge flow to keep one card alive
        // across download + merge phases. Lua shape:
        //   extra_steps = { { key="merge", label="Merge chunks → out.bin" } }
        let mut extra_steps: Vec<(String, String)> = Vec::new();
        if let Ok(Some(arr)) = opts.get::<Option<Table>>("extra_steps") {
            for v in arr.sequence_values::<Table>().flatten() {
                let k = v.get::<Option<String>>("key").ok().flatten().unwrap_or_default();
                let l = v.get::<Option<String>>("label").ok().flatten().unwrap_or_default();
                if !k.is_empty() {
                    extra_steps.push((k, l));
                }
            }
        }
        let keep_open = opt_bool(&opts, "keep_open").unwrap_or(false);

        let res = block_on!(crate::cloud::transfer::download_many(
            h.clone(), conn, bucket, paths, std::path::PathBuf::from(local_dir),
            parallel.unwrap_or(4).clamp(1, 16),
            op_label.unwrap_or_else(|| format!("Downloading items")),
            stream_id.clone(),
            extra_steps,
            keep_open,
        ));
        match res {
            Ok(job_id) => ok2(_lua_ctx, mlua::Value::String(_lua_ctx.create_string(&job_id)?)),
            Err(e)     => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("download_many", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── concat_files ───────────────────────────────────────────────────────────

fn install_concat_files(lua: &Lua, table: &Table) -> Result<()> {
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.concat_files";
        let output = match req_str(&opts, "output", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let inputs_t: Table = match opts.get("inputs") {
            Ok(t)  => t,
            Err(_) => return err2(_lua_ctx, format!("{op}: missing required `inputs` array")),
        };
        let mut inputs: Vec<String> = Vec::new();
        for v in inputs_t.sequence_values::<String>().flatten() {
            inputs.push(v);
        }
        if inputs.is_empty() {
            return err2(_lua_ctx, format!("{op}: `inputs` must contain at least one entry"));
        }
        let delete_inputs = opt_bool(&opts, "delete_inputs").unwrap_or(false);
        let res = block_on!(crate::cloud::ops::concat_files(inputs, output, delete_inputs));
        match res {
            Ok(()) => ok2(_lua_ctx, mlua::Value::Boolean(true)),
            Err(e) => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("concat_files", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── report_progress ────────────────────────────────────────────────────────

// Both report_progress and report_done now drive the OperationsOverlay
// card opened by `download_many` (op_id = `cloud-storage:op:{stream_id}`).
// They replace the dead CloudDownloadProgressModal events used by earlier
// builds. Chunk-handler plugins call these to advance / close the merge
// phase of the same card the download phase started.

fn install_report_progress(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.report_progress";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let stream_id = match req_str(&opts, "stream_id", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let step      = match req_str(&opts, "step",      op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let status    = opt_str(&opts, "status");
        let detail    = opt_str(&opts, "detail");

        let op_id = format!("cloud-storage:op:{stream_id}");
        let kind  = if status.is_some() { "update_step" } else { "set_current" };
        use tauri::Emitter;
        let _ = h.emit("arbor://plugin-operation-update", serde_json::json!({
            "id":     op_id,
            "plugin": "cloud-storage",
            "kind":   kind,
            "step":   step,
            "status": status,
            "detail": detail,
        }));
        ok2(_lua_ctx, mlua::Value::Boolean(true))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("report_progress", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_report_done(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.report_done";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let stream_id = match req_str(&opts, "stream_id", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let ok        = opt_bool(&opts, "ok").unwrap_or(false);
        let summary   = opt_str(&opts, "summary");
        let error     = opt_str(&opts, "error");

        let op_id = format!("cloud-storage:op:{stream_id}");
        use tauri::Emitter;
        let _ = h.emit("arbor://plugin-operation-finish", serde_json::json!({
            "id":      op_id,
            "plugin":  "cloud-storage",
            "summary": summary,
            "error":   error,
        }));

        // Finalize the deferred JobRegistry entry (mirrors `cloud_report_done`
        // command — both share the same `cloud_pending_ops` map).
        use tauri::Manager;
        let state = h.state::<crate::AppState>();
        let job_id = state.cloud_pending_ops.lock().ok()
            .and_then(|mut m| m.remove(&stream_id));
        if let Some(job_id) = job_id {
            let cancelled = state.cloud_cancellations.lock().ok()
                .and_then(|m| m.get(&stream_id).cloned())
                .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(false);
            if let Ok(mut jobs) = state.lock_jobs() {
                let status = if ok {
                    crate::jobs::JobStatus::Completed { exit_code: 0 }
                } else if cancelled {
                    crate::jobs::JobStatus::Cancelled
                } else {
                    crate::jobs::JobStatus::Failed {
                        error: error.clone().unwrap_or_else(|| "merge failed".into()),
                    }
                };
                jobs.set_status(&job_id, status);
            }
            let final_err = if ok {
                None
            } else if cancelled {
                Some("cancelled".to_string())
            } else {
                error.clone().or_else(|| Some("merge failed".into()))
            };
            let _ = h.emit("arbor://job-done", serde_json::json!({
                "job_id":    job_id,
                "success":   ok,
                "exit_code": if ok { 0 } else { -1 },
                "cancelled": cancelled,
                "error":     final_err,
            }));
            if let Ok(mut map) = state.cloud_cancellations.lock() {
                map.remove(&job_id);
                map.remove(&stream_id);
            }
        }
        ok2(_lua_ctx, mlua::Value::Boolean(true))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("report_done", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── pick_chunk_order ───────────────────────────────────────────────────────
// Opens the drag-reorder modal. The plugin supplies items + action; the
// modal fires the action with `{ ok, ordered_paths, ...extra }` on confirm
// or cancel. Mirrors the `arbor.ui.pick_file` round-trip pattern.

fn install_pick_chunk_order(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.pick_chunk_order";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let action = match req_str(&opts, "action", op) {
            Ok(s)  => s,
            Err(e) => return err2(_lua_ctx, e),
        };
        let op_label = opt_str(&opts, "op_label");
        // items + extra → JSON pass-through.
        let raw: serde_json::Value = match _lua_ctx.from_value(mlua::Value::Table(opts)) {
            Ok(v)  => v,
            Err(e) => return err2(_lua_ctx, format!("{op} decode: {e}")),
        };
        let items = raw.get("items").cloned().unwrap_or(serde_json::Value::Array(Vec::new()));
        let extra = raw.get("extra").cloned().unwrap_or(serde_json::Value::Object(Default::default()));

        use tauri::Emitter;
        let payload = serde_json::json!({
            "plugin_name": pname,
            "op_label":    op_label,
            "action":      action,
            "items":       items,
            "extra":       extra,
        });
        let _ = h.emit("arbor://cloud-chunk-order-open", payload);
        ok2(_lua_ctx, mlua::Value::Boolean(true))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("pick_chunk_order", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ── oauth start ────────────────────────────────────────────────────────────

fn install_oauth_start(ctx: &ApiCtx, lua: &Lua, table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let f = lua.create_function(move |_lua_ctx, opts: Table| -> LuaTuple {
        let op = "arbor.cloud.oauth_start";
        let Some(ref h) = handle else { return err2(_lua_ctx, format!("{op}: app handle unavailable")); };
        let secret_ref    = match req_str(&opts, "secret_ref", op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let client_id     = match req_str(&opts, "client_id",  op) { Ok(s) => s, Err(e) => return err2(_lua_ctx, e) };
        let client_secret = opt_str(&opts, "client_secret");
        let res = block_on!(crate::cloud::oauth_google::start(
            h.clone(), secret_ref, client_id, client_secret,
        ));
        match res {
            Ok(url) => ok2(_lua_ctx, mlua::Value::String(_lua_ctx.create_string(&url)?)),
            Err(e)  => err2(_lua_ctx, e.to_string()),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("oauth_start", f).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
