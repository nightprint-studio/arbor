//! `arbor.ui.tree.set(sidebar_id, body)` / `arbor.ui.tree.get(sidebar_id)`.
//!
//! Push a tree snapshot for a sidebar registered with kind="tree". `body`
//! is `{ nodes = {…}, title? = "…" }` or directly an array of nodes. The
//! snapshot is dual-written into the unified ContributionRegistry under
//! point="arbor:tree-state" (item_id=sidebar_id); the frontend reads it
//! via the contribution store and refreshes on the coalesced
//! `arbor://contributions-changed` event.

use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::contrib_write::dual_write_contribution;
use crate::plugin::contribution::points;
use crate::plugin::tree::{BreadcrumbSegment, TreeNode};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let tree_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_set(ctx, lua, &tree_table)?;
    install_get(ctx, lua, &tree_table)?;

    ui.set("tree", tree_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set(ctx: &ApiCtx, lua: &Lua, tree_table: &Table) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let store = ctx.tree_store.clone();
    let handle = ctx.app_handle.clone();
    let contribs_set = ctx.contributions.clone();
    let set_fn = lua.create_function(move |_, (sidebar_id, body): (String, mlua::Value)| {
        // Accept either `{nodes = {...}, title = "..."}` or a bare array of nodes.
        // We avoid moving `body` until we've decided which case applies — using
        // an if-let on a borrow keeps both branches free of borrow/move conflicts.
        let (title_opt, nodes_value, breadcrumb_value, crumb_edit_action, crumb_edit_placeholder, drop_action_opt):
            (Option<String>, mlua::Value, mlua::Value, Option<String>, Option<String>, Option<String>) =
            if let mlua::Value::Table(ref t) = body {
                let title = t.get::<Option<String>>("title").unwrap_or(None);
                let nodes_v = t.get::<mlua::Value>("nodes").unwrap_or(mlua::Value::Nil);
                let crumb_v = t.get::<mlua::Value>("breadcrumb").unwrap_or(mlua::Value::Nil);
                let edit_action = t.get::<Option<String>>("breadcrumb_edit_action").unwrap_or(None);
                let edit_ph     = t.get::<Option<String>>("breadcrumb_edit_placeholder").unwrap_or(None);
                // Phase 6.2 — opt-in drag-drop. When non-nil, the named action
                // is fired with `{source_id, source_data, target_id, target_data}`
                // when a draggable row is dropped on a drop_target row.
                let drop_action_v = t.get::<Option<String>>("drop_action").unwrap_or(None);
                // If `nodes` is missing or nil, treat the whole table as the array.
                let nodes = match nodes_v {
                    mlua::Value::Nil => body.clone(),
                    v => v,
                };
                (title, nodes, crumb_v, edit_action, edit_ph, drop_action_v)
            } else {
                (None, body, mlua::Value::Nil, None, None, None)
            };
        // Breadcrumb is optional — empty array hides the band on the frontend.
        let breadcrumb: Vec<BreadcrumbSegment> = match breadcrumb_value {
            mlua::Value::Nil => Vec::new(),
            v => {
                let j: serde_json::Value =
                    serde_json::to_value(&v).unwrap_or(serde_json::Value::Array(Vec::new()));
                let arr = match j {
                    serde_json::Value::Null => serde_json::Value::Array(Vec::new()),
                    serde_json::Value::Object(ref o) if o.is_empty() => serde_json::Value::Array(Vec::new()),
                    other => other,
                };
                match serde_json::from_value(arr.clone()) {
                    Ok(v)  => v,
                    Err(e) => {
                        tracing::warn!(target: "plugin",
                            "[{}] arbor.ui.tree.set('{}'): breadcrumb deserialization failed: {} (input: {})",
                            pname, sidebar_id, e,
                            {
                                let s = arr.to_string();
                                if s.len() > 400 { format!("{}…", &s[..400]) } else { s }
                            }
                        );
                        Vec::new()
                    }
                }
            }
        };
        let nodes_json: serde_json::Value =
            serde_json::to_value(&nodes_value).unwrap_or(serde_json::Value::Array(Vec::new()));
        // Coerce Lua's "empty table → JSON null/{}" so the top-level
        // array parse doesn't fail when the host pushes an empty list.
        // The per-node `children` field has its own lenient deserializer
        // that handles the same cases element-wise.
        let nodes_json_array = match nodes_json {
            serde_json::Value::Null =>
                serde_json::Value::Array(Vec::new()),
            serde_json::Value::Object(ref o) if o.is_empty() =>
                serde_json::Value::Array(Vec::new()),
            v => v,
        };
        let nodes: Vec<TreeNode> =
            match serde_json::from_value(nodes_json_array.clone()) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(target: "plugin",
                        "[{}] arbor.ui.tree.set('{}'): nodes deserialization failed: {} (input: {})",
                        pname, sidebar_id, e,
                        // Truncate the input so we don't flood logs on huge trees.
                        {
                            let s = nodes_json_array.to_string();
                            if s.len() > 400 { format!("{}…", &s[..400]) } else { s }
                        }
                    );
                    Vec::new()
                }
            };
        // Re-serialize from the deserialized Vec<TreeNode> so the payload
        // shipped to the frontend has empty `children` as `[]` (arrays), not
        // Lua's `{}` (objects). The lenient deserializer above accepts both,
        // but downstream JS consumers iterate `node.children` and would crash
        // on a non-iterable object.
        let nodes_payload = serde_json::to_value(&nodes)
            .unwrap_or(serde_json::Value::Array(Vec::new()));
        let breadcrumb_payload = serde_json::to_value(&breadcrumb)
            .unwrap_or(serde_json::Value::Array(Vec::new()));
        let version = store.set(
            &pname, &sidebar_id, title_opt.clone(),
            breadcrumb, crumb_edit_action.clone(), crumb_edit_placeholder.clone(),
            drop_action_opt.clone(),
            nodes,
        );
        // Dual-write: tree snapshot also goes into the unified
        // ContributionRegistry as a single replace-by-id item. The `version`
        // carries the monotonic ordering for late-arriving updates.
        let payload = serde_json::json!({
            "title":                       title_opt,
            "breadcrumb":                  breadcrumb_payload,
            "breadcrumb_edit_action":      crumb_edit_action,
            "breadcrumb_edit_placeholder": crumb_edit_placeholder,
            "drop_action":                 drop_action_opt,
            "nodes":                       nodes_payload,
            "version":                     version,
        });
        dual_write_contribution(
            &contribs_set, &handle, &pname,
            points::TREE_STATE, &sidebar_id, payload, 100,
        );
        // The coalesced `arbor://contributions-changed` emitted by
        // `dual_write_contribution` is the only event needed — consumers
        // (PluginTreeSidebar, depsExplorerStore) read the snapshot via the
        // unified contribution store keyed on point="arbor:tree-state".
        Ok(version)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    tree_table.set("set", set_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(ctx: &ApiCtx, lua: &Lua, tree_table: &Table) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let store = ctx.tree_store.clone();
    let get_fn = lua.create_function(move |lua_ctx, sidebar_id: String| {
        // Convenience getter — returns the current snapshot, or nil. Useful
        // when a plugin wants to merge contributions into its own snapshot
        // without keeping a parallel cache.
        match store.get(&pname, &sidebar_id) {
            Some(snap) => {
                let json = serde_json::to_value(&snap).unwrap_or(serde_json::Value::Null);
                Ok(lua_ctx.to_value(&json).unwrap_or(mlua::Value::Nil))
            }
            None => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    tree_table.set("get", get_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
