//! `arbor.cloud.*` — the Lua surface the cloud-storage plugin is written against.
//!
//! Lua-visible surface mirrors the shell's original `ns_shell/cloud.rs` byte-for-byte:
//! same namespace (`arbor.cloud`), same function names, same table-config arg
//! shapes, same `(value, nil) | (nil, err)` / `(true|false, err)` tuple
//! conventions, same `arbor.cloud.<op>: …` error prefixes.
//!
//! This is a **PROXY** namespace: the whole cloud stack (the `arbor-cloud`
//! operators, the `ArborCloudHost` bridging into the shell's `JobRegistry` /
//! `PluginHost` / Tauri events / cancellation maps, the OAuth refresher) lives
//! in the **shell** (it is a platform program, earmarked for a WASM runtime).
//! A headless backend can't host it, so every op round-trips over the reverse
//! channel: [`CloudHostOps`] calls `__cloud_<op>` and the matching shell handler
//! in `src-tauri/src/ipc/mod.rs` runs exactly what the shell always ran (same
//! `crate::cloud::{ops,transfer,oauth_google}` calls, same `AppState.cloud_*`
//! maps, same emits). The error `String` is surfaced verbatim to Lua, so the
//! shell handler carries the full text.
//!
//! ## Why this is not a Corvus namespace
//!
//! It used to be one, and that was a claim about the cloud that was never true: an object
//! store has nothing to do with git. What made it Corvus's was that Corvus was the first
//! product with a plugin host — so the day a second one (Bennu) grew one, the cloud panel
//! could not follow, for no reason a user could see. Every op here is a forward to the
//! shell, so the namespace is installable by ANY backend holding a
//! [`HostCaller`](arbor_ipc::prelude::HostCaller): pass one to [`CloudHostOps::new`], hand
//! the installer to `api_installer`, and that product has the cloud.
//!
//! ## ⚠️ Streaming / callback gap
//!
//! Several ops deliver their *results* asynchronously, and the shell delivers them by firing
//! the plugin hook on the product backends (`fire_plugin_hook_on_backends`) — every backend
//! that hosts plugins, so the copy of the plugin that started the op is among them. What the
//! shell does NOT know is which one that was, so a plugin enabled in two products also sees
//! the other's pages; a stream id it never issued is stale by construction and the plugin
//! drops it (which is what `cloud-storage`'s `stream_id` check has always been for).
//!
//! The genuinely one-sided ones are the modal round-trips: `pick_chunk_order` emits
//! `arbor://cloud-chunk-order-open` and the confirm fires `action` back through the shell's
//! own host, and `oauth_start` returns the auth URL inline while the token callback
//! resolves shell-side.
//!
//! `report_progress` / `report_done` are the inverse: a chunk-handler plugin *drives* the
//! shell's OperationsOverlay card + JobRegistry. Those only push state INTO the shell and
//! work whole.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    err2, json_to_lua, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError,
    PluginCoreResult,
};

use crate::cloud::host::CloudHostOps;

/// `arbor.cloud.*` installer. Holds the host handle the closures call through.
pub struct CloudInstaller {
    host: CloudHostOps,
}

impl CloudInstaller {
    pub fn new(host: CloudHostOps) -> Self {
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

fn install_secrets(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let h = host.clone();
    let f = lua
        .create_function(move |lua_ctx, (r, v): (String, String)| -> LuaTuple {
            match h.secret_set(&r, &v) {
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
            match h.secret_exists(&r) {
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
            match h.secret_delete(&r) {
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

fn install_test_connection(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.test_connection";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.test_connection(opts_json) {
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
    host: CloudHostOps,
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
            match host.test_connection_async(opts_json) {
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

fn install_list(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.list";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.list(opts_json) {
                Ok(page) => ok2(lua_ctx, json_to_lua(lua_ctx, &page)?),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("list", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_stat(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.stat";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.stat(opts_json) {
                Ok(o) => ok2(lua_ctx, json_to_lua(lua_ctx, &o)?),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("stat", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_delete(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.delete";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.delete(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("delete", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_copy(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.copy";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.copy(opts_json) {
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

fn install_list_stream(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.list_stream";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            // Returns the `stream_id` (so the plugin can `cancel` it) — but the
            // streamed pages fire into the SHELL's plugin host, not here.
            match host.list_stream(opts_json) {
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

fn install_search_stream(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.search_stream";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.search_stream(opts_json) {
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

fn install_cancel(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, stream_id: String| -> LuaTuple {
            // Best-effort: the shell flips the cancel flag if it exists. Never an error.
            let _ = host.cancel(&stream_id);
            ok2(lua_ctx, mlua::Value::Boolean(true))
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("cancel", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_is_cancelled(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, stream_id: String| -> LuaTuple {
            let cancelled = host.is_cancelled(&stream_id).unwrap_or(false);
            ok2(lua_ctx, mlua::Value::Boolean(cancelled))
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("is_cancelled", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

// ── transfers (return job_id / stream_id) ──────────────────────────────────

fn install_download(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.download";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.download(opts_json) {
                Ok(id) => ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&id)?)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("download", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_upload(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.upload";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.upload(opts_json) {
                Ok(id) => ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&id)?)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("upload", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_sync(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.sync";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            // The `direction` validation (must be "up"/"down") runs shell-side so
            // the `direction must be "up" or "down", got …` error matches verbatim.
            match host.sync(opts_json) {
                Ok(id) => ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&id)?)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("sync", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_download_many(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.download_many";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            // `paths` / `extra_steps` parsing + the empty-paths guard run shell-side
            // (same `\`paths\` must contain at least one entry` text).
            match host.download_many(opts_json) {
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

fn install_concat_files(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.concat_files";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.concat_files(opts_json) {
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

fn install_report_progress(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.report_progress";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.report_progress(opts_json) {
                Ok(()) => ok2(lua_ctx, mlua::Value::Boolean(true)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("report_progress", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_report_done(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.report_done";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.report_done(opts_json) {
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

fn install_pick_chunk_order(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.pick_chunk_order";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            // `action` required-field check runs shell-side. The modal-confirm
            // `action` callback fires into the SHELL's plugin host (gap).
            match host.pick_chunk_order(opts_json) {
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

fn install_oauth_start(host: CloudHostOps, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let f = lua
        .create_function(move |lua_ctx, opts: Table| -> LuaTuple {
            let op = "arbor.cloud.oauth_start";
            let opts_json = match table_to_json(lua_ctx, opts, op) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match host.oauth_start(opts_json) {
                Ok(url) => ok2(lua_ctx, mlua::Value::String(lua_ctx.create_string(&url)?)),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("oauth_start", f)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
