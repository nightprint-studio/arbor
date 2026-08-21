//! `arbor.credentials` — a plugin's own secrets, and only its own.
//!
//! ## What a plugin can reach
//!
//! Exactly the slots its `plugin.toml` declared:
//!
//! ```toml
//! [[credentials]]
//! key   = "oauth"
//! label = "Google account"
//! ```
//!
//! and then `arbor.credentials.set("oauth", token)` / `.get("oauth")` / `.delete("oauth")`.
//!
//! ## What it cannot reach, and why that is structural
//!
//! Arbor's own credentials — git-provider tokens, refresh tokens, issue-tracker keys, the
//! MCP token — live in the same store, and a plugin cannot name one. Not because they are
//! filtered out, but because every name this namespace can produce goes through
//! `arbor_plugin_types::credentials::account_for`, which can only build
//! `plugin/<name>/<key>`. A denylist has gaps that appear the day somebody adds a new kind
//! of Arbor credential and forgets to list it; a namespace has nothing to forget.
//!
//! The declared-slot check and the name are produced by the same call, so there is no order
//! in which a plugin could perform the write without having passed the check.
//!
//! ## Why `list()` returns slots and not values
//!
//! A plugin knows its own slots — they are in its own manifest — so listing them buys
//! nothing except a way to enumerate. What it does return is which slots are **filled**,
//! which is the question a settings panel actually asks ("is this connected?") and the one
//! that can be answered without moving a secret.

use mlua::{Lua, Table};

use arbor_plugin_types::prelude::credential_account_for;

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let t = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let no_host = || {
        mlua::Error::RuntimeError(
            "arbor.credentials: this host has no credential store".to_string(),
        )
    };

    // arbor.credentials.get(key) → string | nil
    {
        let plugin = ctx.plugin_name.clone();
        let slots = ctx.credential_slots.clone();
        let app = ctx.app_ctx.clone();
        let f = lua
            .create_function(move |_, key: String| {
                // Resolved for its side effect as much as its value: this is the call that
                // refuses an undeclared key, and it runs before the host is touched.
                credential_account_for(&plugin, &key, &slots)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                let app = app.as_ref().ok_or_else(no_host)?;
                app.credential_get(&plugin, &key).map_err(mlua::Error::RuntimeError)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("get", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // arbor.credentials.set(key, value)
    {
        let plugin = ctx.plugin_name.clone();
        let slots = ctx.credential_slots.clone();
        let app = ctx.app_ctx.clone();
        let f = lua
            .create_function(move |_, (key, value): (String, String)| {
                credential_account_for(&plugin, &key, &slots)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                // An empty value would store a credential that reads back as "present but
                // blank", which every consumer then has to special-case. Deleting is the
                // operation for "there is no secret here".
                if value.is_empty() {
                    return Err(mlua::Error::RuntimeError(
                        "arbor.credentials.set: value is empty — use delete() to clear a slot"
                            .to_string(),
                    ));
                }
                let app = app.as_ref().ok_or_else(no_host)?;
                app.credential_set(&plugin, &key, &value).map_err(mlua::Error::RuntimeError)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("set", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // arbor.credentials.delete(key)
    {
        let plugin = ctx.plugin_name.clone();
        let slots = ctx.credential_slots.clone();
        let app = ctx.app_ctx.clone();
        let f = lua
            .create_function(move |_, key: String| {
                credential_account_for(&plugin, &key, &slots)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                let app = app.as_ref().ok_or_else(no_host)?;
                app.credential_delete(&plugin, &key).map_err(mlua::Error::RuntimeError)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("delete", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // arbor.credentials.list() → { {key = "oauth", filled = true}, … }
    {
        let plugin = ctx.plugin_name.clone();
        let slots = ctx.credential_slots.clone();
        let app = ctx.app_ctx.clone();
        let f = lua
            .create_function(move |lua, ()| {
                let out = lua.create_table()?;
                for (i, key) in slots.iter().enumerate() {
                    let row = lua.create_table()?;
                    row.set("key", key.as_str())?;
                    // `filled`, not the value. A settings panel asks "is this connected?",
                    // and that question does not need the secret to move.
                    let filled = match app.as_ref() {
                        Some(app) => {
                            app.credential_get(&plugin, key).unwrap_or(None).is_some()
                        }
                        None => false,
                    };
                    row.set("filled", filled)?;
                    out.set(i + 1, row)?;
                }
                Ok(out)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("list", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    arbor.set("credentials", t).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
