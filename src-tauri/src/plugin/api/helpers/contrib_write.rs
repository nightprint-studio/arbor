//! Contribution-write helpers shared by the sugar APIs (sidebar, toolbar,
//! command palette, keybinding, …). All sugar funnels through here so the
//! schema-validate / coalesce-emit logic lives in one place.

use crate::plugin::contribution::{
    ContributionRegistry, PluginContribution, validate_built_in,
};

/// Pushes the payload into the unified ContributionRegistry keyed by
/// (plugin_name, point, item_id) and asks the registry's coalescer to emit
/// `arbor://contributions-changed` so frontend consumers can refetch the
/// affected point. Re-registering with the same key replaces the previous
/// payload (idempotent updates). Bursts of writes to the same point collapse
/// to a single emit (~16ms window).
pub(crate) fn dual_write_contribution(
    contributions: &ContributionRegistry,
    handle:        &Option<tauri::AppHandle>,
    plugin_name:   &str,
    point:         &str,
    item_id:       &str,
    payload:       serde_json::Value,
    priority:      i32,
) {
    // Phase 5 — every built-in point goes through schema validation.
    // Sugar APIs build well-formed payloads, so this should always pass; if
    // it doesn't, the bug is in the API wrapper, not the plugin. We log
    // structurally and drop the write so the bad value never reaches the
    // registry.
    if let Err(e) = validate_built_in(point, &payload) {
        tracing::error!(
            target: "plugin",
            plugin = %plugin_name, point = %point, item_id = %item_id,
            "contribution rejected — schema validation failed: {}", e,
        );
        return;
    }
    contributions.contribute(PluginContribution {
        plugin_name: plugin_name.to_string(),
        point:       point.to_string(),
        item_id:     item_id.to_string(),
        payload,
        priority,
        when:        None,
        disabled:    false,
        group:       None,
    });
    contributions.notify_changed(handle, point);
}

/// Shallow-merge `partial` into the existing contribution payload at
/// `(plugin_name, point, item_id)` and write it back. If no prior payload
/// exists, `partial` becomes the full payload and `default_priority` is used.
/// When a prior payload exists, its priority is preserved (a patch should
/// never silently re-order an item).
pub(crate) fn contribute_patch_payload(
    contributions:    &ContributionRegistry,
    handle:           &Option<tauri::AppHandle>,
    plugin_name:      &str,
    point:            &str,
    item_id:          &str,
    partial:          serde_json::Value,
    default_priority: i32,
) {
    let prior = contributions.list_for_point(point)
        .into_iter()
        .find(|c| c.plugin_name == plugin_name && c.item_id == item_id);
    let (mut payload, priority) = match prior {
        Some(p) => (p.payload, p.priority),
        None    => (serde_json::Value::Object(serde_json::Map::new()), default_priority),
    };
    if let (Some(dst), Some(src)) = (payload.as_object_mut(), partial.as_object()) {
        for (k, v) in src {
            dst.insert(k.clone(), v.clone());
        }
    } else {
        // Non-object prior or non-object partial — fall back to full replace.
        payload = partial;
    }
    dual_write_contribution(contributions, handle, plugin_name, point, item_id, payload, priority);
}

/// Map an `add_toolbar_action({ target = ... })` short-name to the corresponding
/// contribution point. Unknown targets pass through verbatim so plugins can
/// also use this sugar to target custom toolbars they own.
pub(crate) fn toolbar_target_to_point(target: &str) -> String {
    match target {
        "diff"              => "arbor:diff-toolbar".to_string(),
        "status-bar:left"   => "arbor:status-bar:left".to_string(),
        "status-bar:right"  => "arbor:status-bar:right".to_string(),
        "title-bar:left"    => "arbor:title-bar:left".to_string(),
        "title-bar:right"   => "arbor:title-bar:right".to_string(),
        "commit-detail"     => "arbor:commit-detail:action".to_string(),
        "commit-form"       => "arbor:commit-form:action".to_string(),
        "workspace-row"     => "arbor:workspace-row".to_string(),
        other               => other.to_string(),
    }
}
