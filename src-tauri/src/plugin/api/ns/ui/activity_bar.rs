//! Activity-bar contributions:
//!   - arbor.ui.add_graph_combo
//!   - arbor.ui.set_combo_options
//!   - arbor.ui.add_separator

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Lua, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::contrib_write::{
    contribute_patch_payload, dual_write_contribution,
};
use crate::plugin::contribution::points;
use crate::plugin::runtime::ComboOption;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    install_add_graph_combo(ctx, lua, ui)?;
    install_set_combo_options(ctx, lua, ui)?;
    install_add_separator(ctx, lua, ui)?;
    Ok(())
}

fn install_add_graph_combo(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // add_graph_combo — sugar for arbor.ui.contribute("arbor:activitybar", { kind = "combo", … })
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let options = parse_combo_options(&config);

        let id = config.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_graph_combo: 'id' is required".to_string())
        })?;
        let run_action = config.get::<String>("run_action").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.add_graph_combo: 'run_action' is required".to_string())
        })?;
        let select_action = config.get::<Option<String>>("select_action").unwrap_or(None);
        let run_icon      = config.get::<Option<String>>("run_icon").unwrap_or(None);
        let tooltip       = config.get::<Option<String>>("tooltip").unwrap_or(None);
        let target        = config.get::<Option<String>>("target").unwrap_or(None)
                                .unwrap_or_else(|| "activity_bar".to_string());
        let variant       = config.get::<Option<String>>("variant").unwrap_or(None);

        let options_json = serde_json::to_value(&options)
            .unwrap_or(serde_json::Value::Array(Vec::new()));
        let payload = serde_json::json!({
            "kind":          "combo",
            "target":        target,
            "run_action":    run_action,
            "select_action": select_action,
            "run_icon":      run_icon,
            "tooltip":       tooltip,
            "variant":       variant,
            "options":       options_json,
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            points::ACTIVITY_BAR, &id, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("add_graph_combo", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn parse_combo_options(config: &mlua::Table) -> Vec<ComboOption> {
    config
        .get::<Option<mlua::Table>>("options")
        .unwrap_or(None)
        .map(|t| {
            t.sequence_values::<mlua::Table>()
                .filter_map(|r| r.ok())
                .map(|row| ComboOption {
                    value:    row.get::<String>("value").unwrap_or_default(),
                    label:    row.get::<String>("label").unwrap_or_default(),
                    group:    row.get::<Option<String>>("group").unwrap_or(None),
                    color:    row.get::<Option<String>>("color").unwrap_or(None),
                    action:   row.get::<Option<bool>>("action").unwrap_or(None).unwrap_or(false),
                    icon:     row.get::<Option<String>>("icon").unwrap_or(None),
                    subtitle: row.get::<Option<String>>("subtitle").unwrap_or(None),
                    meta:     row.get::<Option<String>>("meta").unwrap_or(None),
                    disabled: row.get::<Option<bool>>("disabled").unwrap_or(None).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn install_set_combo_options(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // set_combo_options{id, options, selected?} → nil
    //
    // Thin sugar over `arbor.ui.contribute_patch`: re-reads the existing
    // combo payload from the contribution registry and patches the
    // `options` field (replace-by-id, other fields preserved).
    //
    // When `selected` is provided AND the value exists in the new
    // options, also emits `plugin:combo-select` so the frontend can adopt
    // the plugin's current pick. This is a separate event because user
    // clicks must NOT trigger a re-sync (the combo payload may not
    // change), and contribution events can fire for other reasons.
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let fn_ = lua.create_function(move |_, cfg: mlua::Table| {
        let id: String = cfg.get("id").map_err(|_|
            mlua::Error::RuntimeError("arbor.ui.set_combo_options: 'id' is required".into()))?;
        let opts_table: mlua::Table = cfg.get("options").map_err(|_|
            mlua::Error::RuntimeError("arbor.ui.set_combo_options: 'options' is required".into()))?;
        let selected_value: Option<String> = cfg.get::<Option<String>>("selected").unwrap_or(None);
        let mut options: Vec<serde_json::Value> = Vec::new();
        for pair in opts_table.sequence_values::<mlua::Table>() {
            if let Ok(row) = pair {
                let value    = row.get::<String>("value").unwrap_or_default();
                let label    = row.get::<String>("label").unwrap_or_default();
                let group    = row.get::<Option<String>>("group").unwrap_or(None);
                let color    = row.get::<Option<String>>("color").unwrap_or(None);
                let action   = row.get::<Option<bool>>("action").unwrap_or(None).unwrap_or(false);
                let icon     = row.get::<Option<String>>("icon").unwrap_or(None);
                let subtitle = row.get::<Option<String>>("subtitle").unwrap_or(None);
                let meta     = row.get::<Option<String>>("meta").unwrap_or(None);
                let disabled = row.get::<Option<bool>>("disabled").unwrap_or(None).unwrap_or(false);
                options.push(serde_json::json!({
                    "value": value, "label": label, "group": group, "color": color,
                    "action": action, "icon": icon, "subtitle": subtitle, "meta": meta,
                    "disabled": disabled,
                }));
            }
        }
        let partial = serde_json::json!({ "options": options.clone() });
        contribute_patch_payload(
            &contribs, &handle, &pname,
            points::ACTIVITY_BAR, &id, partial, 100,
        );
        if let (Some(sv), Some(ref h)) = (selected_value.as_ref(), &handle) {
            if options.iter().any(|o| o.get("value").and_then(|v| v.as_str()) == Some(sv.as_str())) {
                let _ = h.emit("plugin:combo-select", serde_json::json!({
                    "plugin_name": pname, "combo_id": id, "value": sv,
                }));
            }
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("set_combo_options", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_add_separator(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // add_separator()
    //
    // Separators have no plugin-supplied id — we synthesise one from a
    // monotonic counter so multiple `add_separator()` calls within the same
    // plugin produce distinct contributions (replace-by-id would otherwise
    // collapse them all to a single entry).
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
    let sep_counter = Arc::new(AtomicU64::new(0));
    let fn_ = lua.create_function(move |_, ()| {
        let n = sep_counter.fetch_add(1, Ordering::Relaxed);
        let item_id = format!("sep-{}", n);
        let payload = serde_json::json!({
            "kind":   "separator",
            "target": "activity_bar",
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            points::ACTIVITY_BAR, &item_id, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("add_separator", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
