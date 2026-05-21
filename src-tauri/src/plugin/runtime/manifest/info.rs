//! Frontend-facing plugin info + the `[hooks]` / `[sandbox]` manifest sections
//! + `ComboOption` (shared between manifest contributions and the activitybar
//! UI namespace).

use serde::{Deserialize, Serialize};

use super::permissions::PluginPermissions;
use super::schedule::PluginScheduleStatus;

// ---------------------------------------------------------------------------
// Sandbox (Lua stdlib allowlist)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginSandbox {
    /// Standard library modules/functions made available in the sandbox.
    /// Omitting a module removes it entirely. Granular entries like "os.time"
    /// are supported. Defaults to ["string", "table", "math"] when not set.
    #[serde(default = "default_lua_libs")]
    pub lua_libs: Vec<String>,
}

fn default_lua_libs() -> Vec<String> {
    vec![
        "string".into(), "table".into(), "math".into(),
        "os.time".into(), "os.clock".into(), "os.date".into(), "os.difftime".into(),
    ]
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginHooks {
    #[serde(default)] pub on_repo_open:   bool,
    #[serde(default)] pub on_repo_close:  bool,
    #[serde(default)] pub on_repo_init:   bool,
    /// Fired when a repo is permanently removed from Arbor:
    ///   · `delete_registry_repo` — full deregistration, OR
    ///   · `remove_repo_from_workspace` — when the repo was in its last
    ///     workspace AND is not currently open in any tab.
    /// Use this to drop per-repo caches stored outside the repo (e.g.
    /// deps-explorer's tree-cache keyed by absolute paths).
    #[serde(default)] pub on_repo_deregistered: bool,
    #[serde(default)] pub on_plugin_load: bool,
    #[serde(default)] pub on_tab_switch:  bool,
    #[serde(default)] pub on_commit:      bool,
    #[serde(default)] pub on_push:        bool,
    #[serde(default)] pub on_checkout:    bool,
    #[serde(default)] pub on_fetch:       bool,
    // Git Flow lifecycle hooks
    #[serde(default)] pub on_flow_init:           bool,
    #[serde(default)] pub on_flow_feature_start:  bool,
    #[serde(default)] pub on_flow_feature_finish: bool,
    #[serde(default)] pub on_flow_release_start:  bool,
    #[serde(default)] pub on_flow_release_finish: bool,
    #[serde(default)] pub on_flow_hotfix_start:   bool,
    #[serde(default)] pub on_flow_hotfix_finish:  bool,
    // Pipeline lifecycle hooks
    #[serde(default)] pub on_pipeline_started:   bool,
    #[serde(default)] pub on_pipeline_step_done: bool,
    #[serde(default)] pub on_pipeline_done:      bool,
    // Merge Request / Pull Request hooks
    #[serde(default)] pub on_mr_opened:  bool,
    #[serde(default)] pub on_mr_merged:  bool,
    #[serde(default)] pub on_mr_updated: bool,
    // Remote hooks
    #[serde(default)] pub on_pull:           bool,
    // Branch / tag hooks
    #[serde(default)] pub on_branch_create:  bool,
    #[serde(default)] pub on_branch_delete:  bool,
    #[serde(default)] pub on_branch_rename:  bool,
    #[serde(default)] pub on_tag_create:     bool,
    #[serde(default)] pub on_tag_delete:     bool,
    // Stash hooks
    #[serde(default)] pub on_stash_push:     bool,
    #[serde(default)] pub on_stash_pop:      bool,
    // Rebase hooks
    #[serde(default)] pub on_rebase_start:   bool,
    #[serde(default)] pub on_rebase_abort:   bool,
    // Issues (Linear / Jira) hooks
    #[serde(default)] pub on_issue_linked:       bool,
    #[serde(default)] pub on_issue_transitioned: bool,
    // Git Notes hooks
    #[serde(default)] pub on_note_saved:   bool,
    #[serde(default)] pub on_note_deleted: bool,

    // Workspace hooks
    #[serde(default)] pub on_workspace_created:      bool,
    #[serde(default)] pub on_workspace_updated:      bool,
    #[serde(default)] pub on_workspace_deleted:      bool,
    #[serde(default)] pub on_workspace_switched:     bool,
    #[serde(default)] pub on_workspace_repo_added:   bool,
    #[serde(default)] pub on_workspace_repo_removed: bool,

    // Linked Worktrees (cross-project sync) hooks
    #[serde(default)] pub on_worktree_link_sync_started:    bool,
    #[serde(default)] pub on_worktree_link_sync_done:       bool,
    #[serde(default)] pub on_worktree_link_member_added:    bool,
    #[serde(default)] pub on_worktree_link_member_removed:  bool,

    // Theme / branding hooks
    #[serde(default)] pub on_theme_changed: bool,
}

impl PluginHooks {
    /// Returns true if this plugin has declared the named lifecycle hook.
    /// For action hooks (containing ':') and unknown names, always returns true
    /// so they are always routed.
    pub fn subscribes_to(&self, hook: &str) -> bool {
        match hook {
            "on_repo_open"        => self.on_repo_open,
            "on_repo_close"       => self.on_repo_close,
            "on_repo_init"        => self.on_repo_init,
            "on_repo_deregistered"=> self.on_repo_deregistered,
            "on_plugin_load"  => self.on_plugin_load,
            "on_tab_switch"   => self.on_tab_switch,
            "on_commit"       => self.on_commit,
            "on_push"         => self.on_push,
            "on_checkout"     => self.on_checkout,
            "on_fetch"        => self.on_fetch,
            "on_flow_init"           => self.on_flow_init,
            "on_flow_feature_start"  => self.on_flow_feature_start,
            "on_flow_feature_finish" => self.on_flow_feature_finish,
            "on_flow_release_start"  => self.on_flow_release_start,
            "on_flow_release_finish" => self.on_flow_release_finish,
            "on_flow_hotfix_start"   => self.on_flow_hotfix_start,
            "on_flow_hotfix_finish"  => self.on_flow_hotfix_finish,
            "on_pipeline_started"    => self.on_pipeline_started,
            "on_pipeline_step_done"  => self.on_pipeline_step_done,
            "on_pipeline_done"       => self.on_pipeline_done,
            "on_mr_opened"           => self.on_mr_opened,
            "on_mr_merged"           => self.on_mr_merged,
            "on_mr_updated"          => self.on_mr_updated,
            "on_pull"                => self.on_pull,
            "on_branch_create"       => self.on_branch_create,
            "on_branch_delete"       => self.on_branch_delete,
            "on_branch_rename"       => self.on_branch_rename,
            "on_tag_create"          => self.on_tag_create,
            "on_tag_delete"          => self.on_tag_delete,
            "on_stash_push"          => self.on_stash_push,
            "on_stash_pop"           => self.on_stash_pop,
            "on_rebase_start"        => self.on_rebase_start,
            "on_rebase_abort"        => self.on_rebase_abort,
            "on_issue_linked"        => self.on_issue_linked,
            "on_issue_transitioned"  => self.on_issue_transitioned,
            "on_note_saved"          => self.on_note_saved,
            "on_note_deleted"        => self.on_note_deleted,
            "on_workspace_created"      => self.on_workspace_created,
            "on_workspace_updated"      => self.on_workspace_updated,
            "on_workspace_deleted"      => self.on_workspace_deleted,
            "on_workspace_switched"     => self.on_workspace_switched,
            "on_workspace_repo_added"   => self.on_workspace_repo_added,
            "on_workspace_repo_removed" => self.on_workspace_repo_removed,
            "on_worktree_link_sync_started"     => self.on_worktree_link_sync_started,
            "on_worktree_link_sync_done"        => self.on_worktree_link_sync_done,
            "on_worktree_link_member_added"     => self.on_worktree_link_member_added,
            "on_worktree_link_member_removed"   => self.on_worktree_link_member_removed,
            "on_theme_changed"                  => self.on_theme_changed,
            // on_plugin_unload, scheduler-fired action hooks, timer hooks, and
            // generic action hooks are always routed.
            _ => true,
        }
    }
}

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
    pub permissions: PluginPermissions,
    pub hooks:       PluginHooks,
    pub scheduler_count:    usize,
    pub schedulers_running: usize,
    /// Per-action scheduler list with their live running state — used by the
    /// Plugin Info modal to render a toggle per schedule.
    pub schedules:          Vec<PluginScheduleStatus>,
    /// HTML documentation string read from `doc_file`, if declared in plugin.toml.
    pub doc: Option<String>,
    /// Set when the plugin was skipped due to an unmet dependency.
    pub dep_error: Option<String>,
}
