//! Frontend-facing plugin info + `ComboOption` (parsed UI-contribution shape
//! shared between manifest contributions and the activitybar UI namespace).
//!
//! The pure-data shapes that used to live here (`PluginHooks`, `PluginSandbox`)
//! moved to `arbor-plugin-types` (`Hooks`, `Sandbox`). `PluginInfo` is a
//! runtime summary that mixes manifest data with live host state
//! (`enabled`, `schedulers_running`, …) and stays here.

use serde::{Deserialize, Serialize};

use arbor_plugin_types::prelude::{Dependency, Hooks, Permissions, ScheduleStatus};

// ---------------------------------------------------------------------------
// UI registrations (populated at plugin load time by Lua calls)
// ---------------------------------------------------------------------------

/// Combo-button option shape parsed by `add_graph_combo` / `set_combo_options`.
/// Re-used as the parsed form of the `options` array inside the combo's
/// contribution payload (`arbor:activitybar`, `kind = "combo"`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComboOption {
    pub value: String,
    pub label: String,
    pub group: Option<String>,
    /// Optional color hint for profile-variant combos: "dev"|"prod"|"test"|"none".
    #[serde(default)]
    pub color: Option<String>,
    /// When true, picking this option fires `run_action` directly (opens a modal
    /// or similar) and does NOT become the combo's persisted selection. Renders
    /// in a visually separated footer (same pattern as the Workspace dropdown).
    #[serde(default, skip_serializing_if = "is_false")]
    pub action: bool,
    /// Optional Lucide icon name (curated subset — see PluginIcon.LUCIDE_MAP).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Small caption shown under the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Right-aligned muted text (counts, dates, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    /// When true the option renders disabled and cannot be selected.
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
}

fn is_false(b: &bool) -> bool { !*b }

// ---------------------------------------------------------------------------
// PluginInfo — serialisable summary for the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name:        String,
    pub version:     String,
    pub description: String,
    pub author:      String,
    pub license:     Option<String>,
    pub repository:  Option<String>,
    pub keywords:    Vec<String>,
    pub arbor_api:   u32,
    pub enabled:     bool,
    /// Mirrors `experimental` in plugin.toml — surfaced so the Plugin
    /// Manager can render the orange "EXPERIMENTAL" pill on the row.
    #[serde(default)]
    pub experimental: bool,
    pub permissions: Permissions,
    pub hooks:       Hooks,
    pub scheduler_count:    usize,
    pub schedulers_running: usize,
    /// Per-action scheduler list with their live running state — used by the
    /// Plugin Info modal to render a toggle per schedule.
    pub schedules:          Vec<ScheduleStatus>,
    /// HTML documentation string read from `doc_file`, if declared in plugin.toml.
    pub doc: Option<String>,
    /// Set when the plugin was skipped due to an unmet dependency.
    pub dep_error: Option<String>,
    /// Direct declared dependencies from the manifest. Surfaced in the
    /// Plugin Manager detail pane so the user can see at a glance what a
    /// plugin needs without having to open the dependency-graph modal.
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// Names of installed plugins (loaded or dormant) that declare this one
    /// as a required dependency. Used by the detail pane's "Required by" row
    /// and by the cascade-confirm modal.
    #[serde(default)]
    pub required_by: Vec<String>,
}
