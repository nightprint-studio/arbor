//! Contribution sugar APIs:
//!   - arbor.ui.add_context_menu_item
//!   - arbor.ui.add_menu_item
//!   - arbor.ui.add_toolbar_action
//!   - arbor.ui.add_sidebar
//!   - arbor.ui.set_panel_content
//!
//! Each is a thin wrapper that builds a payload and funnels it through
//! `dual_write_contribution` keyed at the appropriate point.

use mlua::{Lua, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::contrib_write::{
    dual_write_contribution, toolbar_target_to_point,
};
use crate::plugin::contribution::points;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    install_add_context_menu_item(ctx, lua, ui)?;
    install_add_menu_item(ctx, lua, ui)?;
    install_add_toolbar_action(ctx, lua, ui)?;
    install_add_sidebar(ctx, lua, ui)?;
    install_set_panel_content(ctx, lua, ui)?;
    Ok(())
}

fn install_add_context_menu_item(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // add_context_menu_item — sugar for arbor.ui.contribute("arbor:context-menu:<target>", …)
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let target = config.get::<String>("target").unwrap_or_else(|_| "commit".to_string());
        let label  = config.get::<String>("label").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_context_menu_item: 'label' is required".to_string())
        })?;
        let action = config.get::<String>("action").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_context_menu_item: 'action' is required".to_string())
        })?;
        let icon: Option<String> = config.get::<Option<String>>("icon").unwrap_or(None);
        // The point is per-target so consumers (commit menu, branch menu,
        // file menu, …) subscribe to their slot without runtime filtering.
        // `target` stays in the payload for introspection.
        // item_id = action name (a plugin's actions are unique within itself).
        let point = points::context_menu_point(&target);
        let payload = serde_json::json!({
            "target": target,
            "label":  label,
            "action": action,
            "icon":   icon,
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            &point, &action, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("add_context_menu_item", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_add_menu_item(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // add_menu_item — sugar for arbor.ui.contribute("arbor:menu", …)
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let label  = config.get::<String>("label").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_menu_item: 'label' is required".to_string())
        })?;
        let action = config.get::<String>("action").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_menu_item: 'action' is required".to_string())
        })?;
        let icon: Option<String> = config.get::<Option<String>>("icon").unwrap_or(None);
        let payload = serde_json::json!({
            "label":  label,
            "action": action.clone(),
            "icon":   icon,
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            points::MENU, &action, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("add_menu_item", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_add_toolbar_action(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // add_toolbar_action — generic sugar for any of Arbor's inline-action
    // toolbars. `target` is one of the well-known short names (see
    // `toolbar_target_to_point`). Unknown targets are forwarded verbatim,
    // so plugins can target their own custom toolbars through the same API.
    //
    //   config = {
    //     id      = string,           -- required, unique within (plugin, target)
    //     target  = string,           -- required
    //     action  = string,           -- required, plugin action fired on click
    //     label   = string|nil,       -- optional, omit for icon-only buttons
    //     icon    = string|nil,       -- optional, Lucide name or emoji
    //     tooltip = string|nil,       -- optional
    //     color   = string|nil,       -- optional
    //   }
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let id = config.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_toolbar_action: 'id' is required".to_string())
        })?;
        let target = config.get::<String>("target").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_toolbar_action: 'target' is required".to_string())
        })?;
        let action = config.get::<String>("action").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_toolbar_action: 'action' is required".to_string())
        })?;
        let label   = config.get::<Option<String>>("label").unwrap_or(None);
        let icon    = config.get::<Option<String>>("icon").unwrap_or(None);
        let tooltip = config.get::<Option<String>>("tooltip").unwrap_or(None);
        let color   = config.get::<Option<String>>("color").unwrap_or(None);

        let point = toolbar_target_to_point(&target);
        let payload = serde_json::json!({
            "label":   label,
            "icon":    icon,
            "action":  action,
            "tooltip": tooltip,
            "color":   color,
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            &point, &id, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("add_toolbar_action", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_add_sidebar(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // add_sidebar — unified API, plugins pick both `side` and `position`.
    //   side:     "left" | "right"  (default "right" — plugins extend the right bar)
    //   position: "top"  | "bottom" (default "top" — classic sidebar panel)
    // The plugin provides an `id` (unique within the plugin). Clicking the icon
    // fires the hook `panel:open:<id>` on the plugin; the plugin is expected to
    // respond with `arbor.ui.set_panel_content(id, {title, nodes})`.
    // Sugar for arbor.ui.contribute("arbor:sidebar", …)
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, config: mlua::Table| {
        let id = config.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_sidebar: 'id' is required".to_string())
        })?;
        let side = config.get::<Option<String>>("side")
            .unwrap_or(None).unwrap_or_else(|| "right".to_string());
        let position = config.get::<Option<String>>("position")
            .unwrap_or(None).unwrap_or_else(|| "top".to_string());
        let action = config.get::<Option<String>>("action")
            .unwrap_or(None)
            // When the plugin doesn't provide a classic `action`, fall back to
            // a deterministic `panel:open:<id>` action name. The frontend
            // uses `panel:open:<id>` consistently anyway.
            .unwrap_or_else(|| format!("panel:open:{}", id));
        // `kind` selects how the panel body is rendered:
        //   "form" (default) — pushed via set_panel_content (form DSL)
        //   "tree"           — pushed via arbor.ui.tree.set; renders with
        //                      PluginTreeSidebar (header/body/footer + shared Tree widget).
        let kind = config.get::<Option<String>>("kind")
            .unwrap_or(None).unwrap_or_else(|| "form".to_string());
        let label = config.get::<String>("label").unwrap_or_else(|_| id.clone());
        let icon: Option<String>    = config.get::<Option<String>>("icon").unwrap_or(None);
        let collapsable             = config.get::<bool>("collapsable").unwrap_or(false);
        let tooltip: Option<String> = config.get::<Option<String>>("tooltip").unwrap_or(None);
        // Optional `search` config — when present, drives PluginTreeSidebar's
        // search row. Shape (any subset):
        //   { modes = {"local","remote"}, default = "local",
        //     remote_action = "<plugin-action>",
        //     placeholder_local = "...", placeholder_remote = "...",
        //     wildcard_hint = true }
        // Pass through as opaque JSON; the parser on the frontend validates.
        let search_value: serde_json::Value = match config.get::<mlua::Value>("search") {
            Ok(mlua::Value::Table(t)) => {
                use mlua::LuaSerdeExt;
                lua_ctx.from_value(mlua::Value::Table(t)).unwrap_or(serde_json::Value::Null)
            }
            _ => serde_json::Value::Null,
        };
        let payload = serde_json::json!({
            "action":      action,
            "label":       label,
            "icon":        icon,
            "collapsable": collapsable,
            "side":        side,
            "position":    position,
            "tooltip":     tooltip,
            "kind":        kind,
            "search":      search_value,
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            points::SIDEBAR, &id, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("add_sidebar", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_panel_content(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // set_panel_content(id, {title, nodes, actions?})
    //
    // Sugar over `arbor.ui.contribute("arbor:panel-content", {id, …})`. Each
    // call is a full replace of the panel body (no merge — title/nodes/actions
    // are atomic together). The frontend re-renders via
    // `arbor://contributions-changed` from the contribution registry.
    let pname = ctx.plugin_name.clone();
    let handle = ctx.app_handle.clone();
    let contribs = ctx.contributions.clone();
    let fn_ = lua.create_function(move |_, (id, body): (String, mlua::Table)| {
        let nodes_val = body.get::<mlua::Value>("nodes").unwrap_or(mlua::Value::Nil);
        let actions_val = body.get::<mlua::Value>("actions").unwrap_or(mlua::Value::Nil);
        let json_body: serde_json::Value =
            serde_json::to_value(&nodes_val).unwrap_or(serde_json::Value::Null);
        let json_actions: serde_json::Value =
            serde_json::to_value(&actions_val).unwrap_or(serde_json::Value::Null);
        let title: Option<String> = body.get::<Option<String>>("title").unwrap_or(None);
        let payload = serde_json::json!({
            "title":   title,
            "nodes":   json_body,
            "actions": json_actions,
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            points::PANEL_CONTENT, &id, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("set_panel_content", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
