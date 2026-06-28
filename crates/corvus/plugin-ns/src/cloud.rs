//! `arbor.cloud.*` — Lua surface for the cloud-storage plugin, ported to run
//! through an [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface mirrors the shell's `ns_shell/cloud.rs` byte-for-byte:
//! same namespace (`arbor.cloud`), same function names, same table-config arg
//! shapes, same `(value, nil) | (nil, err)` / `(true|false, err)` tuple
//! conventions, same `arbor.cloud.<op>: …` error prefixes.
//!
//! This is a **PROXY** namespace: the whole cloud stack (the `arbor-cloud`
//! operators, the `ArborCloudHost` bridging into the shell's `JobRegistry` /
//! `PluginHost` / Tauri events / cancellation maps, the OAuth refresher) lives
//! in the **shell** (it is a platform program, earmarked for a WASM runtime).
//! `corvus-be` can't host it, so every op round-trips over the reverse channel:
//! the `CorvusNsHost` impl calls `host_call("__cloud_<op>", …)` and the matching
//! shell handler in `src-tauri/src/ipc/mod.rs` runs exactly what
//! `ns_shell/cloud.rs` ran (same `crate::cloud::{ops,transfer,oauth_google}`
//! calls, same `AppState.cloud_*` maps, same emits). The error `String` is
//! surfaced verbatim to Lua, so the shell handler carries the full text.
//!
//! ## ⚠️ Streaming / callback gap
//!
//! Several ops deliver their *results* asynchronously back into the **plugin
//! host that started them** — and for a `corvus-be` plugin that host is the
//! corvus-be plugin host, NOT the shell's. The proxy only forwards the *start*;
//! the asynchronous tail (streamed pages, the async test reply, the
//! reorder-modal confirmation) fires inside the SHELL's plugin host / FE and
//! never reaches the corvus-be plugin that called the op. Concretely:
//!
//! - `list_stream` / `search_stream` — the shell's `ArborCloudHost::fire_plugin_hook`
//!   fires the per-page `on_*` hooks on the SHELL's plugins. A corvus-be plugin
//!   gets the `stream_id` back and can `cancel` it, but never receives the
//!   streamed pages. **Degraded to fire-and-forget.**
//! - `test_connection_async` — the async reply is fired via `fire_broadcast`
//!   into the SHELL's plugin host under `on_done`; a corvus-be subscriber never
//!   sees it. (The synchronous `test_connection` works fully — it returns inline.)
//! - `pick_chunk_order` — emits `arbor://cloud-chunk-order-open`; the modal's
//!   confirm fires the `action` back through the SHELL's plugin host.
//! - `oauth_start` — returns the auth URL inline (works), but the eventual
//!   token-callback resolves inside the shell.
//!
//! `report_progress` / `report_done` are the inverse: a chunk-handler plugin
//! *drives* the shell's OperationsOverlay card + JobRegistry. Those are proxied
//! whole and work (they only push state INTO the shell).

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    err2, json_to_lua, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError,
    PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// `arbor.cloud.*` installer. Holds the host handle the closures call through.
pub struct CloudInstaller {
    host: NsHostHandle,
}

impl CloudInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for CloudInstaller {
    fn install(&self, _ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let t = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_secrets(self.host.clone(), lua, &t)?;
        install_test_connection(self.host.clone(), lua, &t)?;
        install_test_connection_async(self.host.clone(), lua, &t)?;
        install_list(self.host.clone(), lua, &t)?;
        install_list_stream(self.host.clone(), lua, &t)?;
        install_search_stream(self.host.clone(), lua, &t)?;
        install_cancel(self.host.clone(), lua, &t)?;
        install_is_cancelled(self.host.clone(), lua, &t)?;
        install_stat(self.host.clone(), lua, &t)?;
        install_delete(self.host.clone(), lua, &t)?;
        install_copy(self.host.clone(), lua, &t)?;
        install_download(self.host.clone(), lua, &t)?;
        install_upload(self.host.clone(), lua, &t)?;
        install_sync(self.host.clone(), lua, &t)?;
        install_download_many(self.host.clone(), lua, &t)?;
        install_concat_files(self.host.clone(), lua, &t)?;
        install_report_progress(self.host.clone(), lua, &t)?;
        install_report_done(self.host.clone(), lua, &t)?;
        install_pick_chunk_order(self.host.clone(), lua, &t)?;
        install_oauth_start(self.host.clone(), lua, &t)?;

        arbor
            .set("cloud", t)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

// ── marshalling helpers ─────────────────────────────────────────────────────

/// Serialize a Lua table to JSON via serde so the host receives the same envelope
/// the shell's `ns_shell/cloud.rs` deserialized (`conn`, op args, `paths`, …).
/// A decode failure carries the op-prefixed message verbatim (→ Lua `(nil, err)`).
fn table_to_json(lua: &Lua, t: Table, op: &str) -> std::result::Result<serde_json::Value, String> {
    lua.from_value(mlua::Value::Table(t))
        .map_err(|e| format!("{op}: decode opts: {e}"))
}

// ── secrets ────────────────────────────────────────────────────────────────

fn install_secrets(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let h = host.clone();
    let f = lua
        .create_function(move |lua_ctx, (r, v): (String, String)| -> LuaTuple {
            match h.cloud_secret_set(&r, &v) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("secret_set", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let h = host.clone();
    let f = lua
        .create_function(move |lua_ctx, r: String| -> LuaTuple {
            match h.cloud_secret_exists(&r) {
                Ok(b) => ok2(lua_ctx, mlua::Value::Boolean(b)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("secret_exists", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let h = host.clone();
    let f = lua
        .create_function(move |lua_ctx, r: String| -> LuaTuple {
            match h.cloud_secret_delete(&r) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("secret_delete", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── test_connection ────────────────────────────────────────────────────────

fn install_test_connection(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.test_connection";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_test_connection(opts_json) {
                Ok(reply) => ok2(lua_ctx, json_to_lua(lua_ctx, &reply)?),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("test_connection", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// Non-blocking variant. The async reply fires into the SHELL's plugin host
/// (see the module-level streaming-callback gap note) — a corvus-be subscriber
/// won't see it. Forwarded for surface fidelity; returns `true` immediately.
fn install_test_connection_async(
    host: NsHostHandle,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.test_connection_async";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_test_connection_async(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("test_connection_async", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── list / stat / delete / copy ────────────────────────────────────────────

fn install_list(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.list";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_list(opts_json) {
                Ok(page) => ok2(lua_ctx, json_to_lua(lua_ctx, &page)?),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("list", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_stat(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.stat";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_stat(opts_json) {
                Ok(o) => ok2(lua_ctx, json_to_lua(lua_ctx, &o)?),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("stat", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_delete(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.delete";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_delete(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("delete", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_copy(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.copy";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_copy(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("copy", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── streams (cancel-driven; per-page events fire shell-side) ────────────────

fn install_list_stream(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.list_stream";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            // Returns the `stream_id` (so the plugin can `cancel` it) — but the
            // streamed pages fire into the SHELL's plugin host, not here.
            match host.cloud_list_stream(opts_json) {
                Ok(stream_id) => {
                    ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&stream_id)?))
                }
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("list_stream", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_search_stream(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.search_stream";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_search_stream(opts_json) {
                Ok(stream_id) => {
                    ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&stream_id)?))
                }
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("search_stream", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_cancel(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, stream_id: String| -> LuaTuple {
            // Best-effort: the shell flips the cancel flag if it exists. Never an error.
            let _ = host.cloud_cancel(&stream_id);
            ok2(lua_ctx, mlua::Value::Boolean(true))
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("cancel", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_is_cancelled(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, stream_id: String| -> LuaTuple {
            let cancelled = host.cloud_is_cancelled(&stream_id).unwrap_or(false);
            ok2(lua_ctx, mlua::Value::Boolean(cancelled))
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("is_cancelled", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── transfers (return job_id / stream_id) ──────────────────────────────────

fn install_download(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.download";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_download(opts_json) {
                Ok(id) => ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&id)?)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("download", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_upload(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.upload";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_upload(opts_json) {
                Ok(id) => ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&id)?)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("upload", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_sync(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.sync";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            // The `direction` validation (must be "up"/"down") runs shell-side so
            // the `direction must be "up" or "down", got …` error matches verbatim.
            match host.cloud_sync(opts_json) {
                Ok(id) => ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&id)?)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("sync", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_download_many(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.download_many";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            // `paths` / `extra_steps` parsing + the empty-paths guard run shell-side
            // (same `\`paths\` must contain at least one entry` text).
            match host.cloud_download_many(opts_json) {
                Ok(job_id) => {
                    ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&job_id)?))
                }
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("download_many", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── concat_files ───────────────────────────────────────────────────────────

fn install_concat_files(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.concat_files";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_concat_files(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("concat_files", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── report_progress / report_done (push state INTO the shell — work fully) ──

fn install_report_progress(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.report_progress";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_report_progress(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("report_progress", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_report_done(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.report_done";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_report_done(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("report_done", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── pick_chunk_order (emits the modal; confirm fires shell-side) ────────────

fn install_pick_chunk_order(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.pick_chunk_order";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            // `action` required-field check runs shell-side. The modal-confirm
            // `action` callback fires into the SHELL's plugin host (gap).
            match host.cloud_pick_chunk_order(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("pick_chunk_order", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── oauth start (returns the URL inline; token-callback resolves shell-side) ─

fn install_oauth_start(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.oauth_start";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.cloud_oauth_start(opts_json) {
                Ok(url) => ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&url)?)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("oauth_start", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
