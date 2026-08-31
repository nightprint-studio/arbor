//! `arbor.oauth` — the OAuth machinery a plugin cannot own, driven by a plugin that does.
//!
//! ## What the host supplies, and what it refuses to know
//!
//! An installed-app flow has exactly two parts a package cannot hold. The **loopback
//! listener**: the browser redirects to `http://127.0.0.1:<port>/` carrying the authorization
//! code, and neither a Lua plugin nor a wasm guest can bind a socket — nor should, since that
//! socket is where the code lands. And the **keychain**: the tokens belong in the OS store,
//! under the plugin's own credential namespace, which Arbor brokers.
//!
//! Everything else is data the caller passes: endpoints, client, scopes, and whatever dialect
//! the provider insists on. That is the line — Arbor runs the flow, the plugin knows the
//! provider. No endpoint of anybody's is written down in Arbor.
//!
//! ```lua
//! local url, err = arbor.oauth.start{
//!   slot          = "oauth",                       -- one of your [[credentials]] slots
//!   auth_url      = "https://accounts.example/o/oauth2/v2/auth",
//!   token_url     = "https://oauth2.example/token",
//!   client_id     = cfg.client_id,
//!   client_secret = cfg.client_secret,             -- optional
//!   scope         = { "https://example/auth/storage.read_write" },
//!   redirect_port = 7732,
//!   extra_params  = { access_type = "offline", prompt = "consent" },
//!   label         = "Example Storage",
//!   on_done       = "myplugin:oauth-done",         -- fired with { ok, error? }
//! }
//! arbor.ui.open_url(url)
//! ```
//!
//! `start` returns as soon as there is a URL to open — it never waits for the person. The
//! outcome arrives as the `on_done` hook, on this plugin's own host.
//!
//! ```lua
//! -- Before a request: renew only if the stored token is nearly out.
//! local r = arbor.oauth.refresh{
//!   slot = "oauth", token_url = "…", min_remaining_secs = 60,
//! }
//! -- r.refreshed == false  → the stored one was still good
//! ```
//!
//! ## What ends up in the slot
//!
//! A JSON document: `refresh_token`, `access_token`, `expires_at`, and the `client_id` /
//! `client_secret` the tokens were issued to. It is a documented shape because it has a second
//! reader — a **provider extension** reads `access_token` out of the same slot through
//! `arbor:host/secrets`, which is how a guest authenticates without ever being part of the
//! flow.
//!
//! ## The gate
//!
//! The slot must be one this plugin declared in `[[credentials]]`. The check is the same call
//! that builds the account name (`credential_account_for`), so there is no order in which a
//! plugin could reach the store without having passed it — the namespace cannot even spell
//! another package's slot, let alone Arbor's own credentials.

use mlua::{Lua, LuaSerdeExt, Table, Value};

use arbor_plugin_types::prelude::credential_account_for;

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let t = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let no_host =
        || mlua::Error::RuntimeError("arbor.oauth: this host has no OAuth engine".to_string());

    // arbor.oauth.start{ slot, auth_url, token_url, client_id, … } → url
    {
        let plugin = ctx.plugin_name.clone();
        let slots = ctx.credential_slots.clone();
        let app = ctx.app_ctx.clone();
        let f = lua
            .create_function(move |lua, spec: Table| {
                let json = prepare(lua, &spec, &plugin, &slots, "arbor.oauth.start")?;
                let app = app.as_ref().ok_or_else(no_host)?;
                app.oauth_start(&plugin, &json).map_err(mlua::Error::RuntimeError)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("start", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // arbor.oauth.refresh{ slot, token_url, min_remaining_secs? } → { refreshed, expires_in }
    {
        let plugin = ctx.plugin_name.clone();
        let slots = ctx.credential_slots.clone();
        let app = ctx.app_ctx.clone();
        let f = lua
            .create_function(move |lua, spec: Table| {
                let json = prepare(lua, &spec, &plugin, &slots, "arbor.oauth.refresh")?;
                let app = app.as_ref().ok_or_else(no_host)?;
                let out =
                    app.oauth_refresh(&plugin, &json).map_err(mlua::Error::RuntimeError)?;
                let v: serde_json::Value = serde_json::from_str(&out)
                    .map_err(|e| mlua::Error::RuntimeError(format!("arbor.oauth.refresh: {e}")))?;
                lua.to_value(&v)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("refresh", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    arbor.set("oauth", t).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// Check the slot, normalise the two fields Lua spells differently from the wire, and encode.
///
/// Both entry points do the same three things and differ only in which host method they then
/// call, so this is all of it in one place — including the gate, which must run before the
/// spec can reach a host that would act on it.
fn prepare(
    lua: &Lua,
    spec: &Table,
    plugin: &str,
    slots: &[String],
    who: &str,
) -> mlua::Result<String> {
    let slot: String = spec
        .get("slot")
        .map_err(|_| mlua::Error::RuntimeError(format!("{who}: `slot` is required")))?;
    // Resolved for its side effect as much as its value: this is the call that refuses a slot
    // the manifest never declared, and it runs before the host is touched.
    credential_account_for(plugin, &slot, slots)
        .map_err(|e| mlua::Error::RuntimeError(format!("{who}: {e}")))?;

    let mut json = match lua
        .from_value::<serde_json::Value>(Value::Table(spec.clone()))
        .map_err(|e| mlua::Error::RuntimeError(format!("{who}: {e}")))?
    {
        serde_json::Value::Object(map) => map,
        other => {
            return Err(mlua::Error::RuntimeError(format!(
                "{who}: expected a config table, got {other}"
            )))
        }
    };

    if let Some(scope) = json.get("scope").cloned() {
        json.insert("scope".into(), serde_json::Value::String(scope_string(&scope)));
    }
    if let Some(extra) = json.get("extra_params").cloned() {
        json.insert("extra_params".into(), pairs(&extra));
    }

    Ok(serde_json::Value::Object(json).to_string())
}

/// Scopes as the spec writes them: one space-separated string.
///
/// A Lua author reaches for a list, and a provider's docs quote a single string; accepting
/// both costs three lines and removes a class of "why is my scope rejected" that has nothing
/// to do with OAuth.
fn scope_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Extra authorize parameters as ordered pairs.
///
/// A Lua table is the natural way to write them (`{ access_type = "offline" }`) and arrives as
/// a JSON object; the flow takes pairs, because a query string is ordered. A caller who cares
/// about the order writes a list of pairs instead, and that passes through untouched.
fn pairs(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => serde_json::Value::Array(
            map.iter()
                .map(|(k, val)| {
                    serde_json::json!([k, val.as_str().map(|s| s.to_string()).unwrap_or_else(|| val.to_string())])
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_arrive_as_a_list_or_as_a_string() {
        assert_eq!(scope_string(&serde_json::json!(["a", "b"])), "a b");
        assert_eq!(scope_string(&serde_json::json!("a b")), "a b");
    }

    #[test]
    fn extra_params_written_as_a_table_become_pairs() {
        let out = pairs(&serde_json::json!({ "access_type": "offline" }));
        assert_eq!(out, serde_json::json!([["access_type", "offline"]]));
    }

    #[test]
    fn extra_params_written_as_pairs_keep_their_order() {
        // A provider that requires two parameters in a given order gets to say so, and this
        // must not "helpfully" re-key them through a map.
        let given = serde_json::json!([["b", "2"], ["a", "1"]]);
        assert_eq!(pairs(&given), given);
    }

    #[test]
    fn a_non_string_extra_value_still_crosses_as_text() {
        // A query parameter is text. A number written in Lua is a number in JSON, and dropping
        // it would be worse than stringifying it.
        let out = pairs(&serde_json::json!({ "n": 7 }));
        assert_eq!(out, serde_json::json!([["n", "7"]]));
    }
}
