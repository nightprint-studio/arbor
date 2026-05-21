//! Static catalog of built-in hooks with their context schema.
//!
//! Each entry documents a hook fired by the host: its name, a one-line
//! description, the category it belongs to (for grouping in docs), and the
//! shape of the `ctx` table the handler receives.
//!
//! The catalog is exposed to plugins via `arbor.hooks.list()` and
//! `arbor.hooks.describe(name)` so they can discover what's available and
//! what fields each hook payload carries — without having to consult external
//! documentation.

#[derive(Copy, Clone)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    StringArray,
    Object,
}

impl FieldType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::String      => "string",
            Self::Number      => "number",
            Self::Boolean     => "boolean",
            Self::StringArray => "string[]",
            Self::Object      => "object",
        }
    }
}

#[derive(Copy, Clone)]
pub struct HookField {
    pub name:        &'static str,
    pub ty:          FieldType,
    pub required:    bool,
    pub description: &'static str,
}

#[derive(Copy, Clone)]
pub struct HookDef {
    pub name:        &'static str,
    pub category:    &'static str,
    pub description: &'static str,
    pub ctx:         &'static [HookField],
}

// Helper macro: keeps each entry compact and readable.
macro_rules! field {
    ($name:literal, $ty:ident, req, $desc:literal) => {
        HookField { name: $name, ty: FieldType::$ty, required: true,  description: $desc }
    };
    ($name:literal, $ty:ident, opt, $desc:literal) => {
        HookField { name: $name, ty: FieldType::$ty, required: false, description: $desc }
    };
}

const NO_CTX: &[HookField] = &[];

// Common context shapes — defined once, referenced by multiple hooks.
const TAB_PATH_NAME_CTX: &[HookField] = &[
    field!("tab_id", String, req, "Tab id of the affected repo."),
    field!("path",   String, req, "Absolute path of the repo on disk."),
    field!("name",   String, req, "Display name of the repo."),
];

pub static HOOK_CATALOG: &[HookDef] = &[
    // ── Lifecycle ──────────────────────────────────────────────────────────
    HookDef {
        name: "on_plugin_load",
        category: "lifecycle",
        description: "Fired once after the plugin's main.lua finishes executing. Use it as the plugin constructor.",
        ctx: NO_CTX,
    },
    HookDef {
        name: "on_plugin_unload",
        category: "lifecycle",
        description: "Fired before the plugin is unloaded (reload, disable, app shutdown). Use it to release resources.",
        ctx: NO_CTX,
    },

    // ── Repo / project ─────────────────────────────────────────────────────
    HookDef {
        name: "on_repo_open",
        category: "repo",
        description: "Fired when the user opens a repo (new tab or after a plugin reload).",
        ctx: TAB_PATH_NAME_CTX,
    },
    HookDef {
        name: "on_repo_close",
        category: "repo",
        description: "Fired when the user closes a repo tab.",
        ctx: TAB_PATH_NAME_CTX,
    },
    HookDef {
        name: "on_repo_init",
        category: "repo",
        description: "Fired when a non-git folder is initialised as a repo via Arbor's Init flow.",
        ctx: &[
            field!("path",           String,  req, "Absolute path of the new repo."),
            field!("name",           String,  req, "Display name of the new repo."),
            field!("default_branch", String,  req, "Initial branch name (e.g. 'main')."),
            field!("provider",       String,  req, "Hosting provider chosen by the user (github/gitlab/custom/none)."),
            field!("remote_url",     String,  opt, "Remote URL — empty if no remote was configured."),
            field!("pushed",         Boolean, req, "True if the initial commit was pushed to the remote."),
            field!("has_readme",     Boolean, req, "True if a README was generated."),
            field!("license",        String,  opt, "License identifier (e.g. 'mit') or empty."),
        ],
    },
    HookDef {
        name: "on_repo_deregistered",
        category: "repo",
        description: "Fired when a repo is permanently removed from Arbor (registry deletion, or removed from its last workspace and not open in any tab). Use it to drop per-repo caches.",
        ctx: &[
            field!("repo_id", String, req, "Stable repo identifier."),
            field!("path",    String, req, "Last known absolute path."),
            field!("name",    String, req, "Display name."),
            field!("reason",  String, req, "Why the repo was deregistered (e.g. 'registry_delete', 'removed_from_last_workspace')."),
        ],
    },
    HookDef {
        name: "on_project_missing",
        category: "repo",
        description: "Fired when a registered project's path is no longer valid on disk (deleted, moved, drive offline) at open time.",
        ctx: &[
            field!("repo_id", String, req, "Stable repo identifier."),
            field!("path",    String, req, "Path that failed validation."),
            field!("name",    String, req, "Display name."),
            field!("reason",  String, req, "Reason the path is invalid."),
        ],
    },
    HookDef {
        name: "on_project_relocated",
        category: "repo",
        description: "Fired when the user picks a new on-disk location for a missing project via the Locate flow. Plugins keyed off the absolute path should rebase their bookkeeping.",
        ctx: &[
            field!("repo_id",    String, req, "Stable repo identifier."),
            field!("old_path",   String, req, "Previous (now invalid) path."),
            field!("new_path",   String, req, "New on-disk path."),
            field!("name",       String, req, "Display name."),
            field!("remote_url", String, opt, "Remote URL — empty if no remote configured."),
        ],
    },
    HookDef {
        name: "on_tab_switch",
        category: "repo",
        description: "Fired when the user activates a different repo tab.",
        ctx: TAB_PATH_NAME_CTX,
    },

    // ── Branch / tag ───────────────────────────────────────────────────────
    HookDef {
        name: "on_branch_create",
        category: "branch",
        description: "Fired after a new local branch is created.",
        ctx: &[
            field!("tab_id",   String, req, "Tab id of the affected repo."),
            field!("name",     String, req, "Branch name."),
            field!("from_oid", String, req, "Commit oid the branch was created from."),
        ],
    },
    HookDef {
        name: "on_branch_delete",
        category: "branch",
        description: "Fired after one or more local branches are deleted. Single-branch deletes carry `name`; bulk deletes carry `names`.",
        ctx: &[
            field!("tab_id", String,      req, "Tab id of the affected repo."),
            field!("name",   String,      opt, "Branch name (single-delete variant)."),
            field!("names",  StringArray, opt, "Branch names (bulk-delete variant)."),
        ],
    },
    HookDef {
        name: "on_branch_rename",
        category: "branch",
        description: "Fired after a local branch is renamed.",
        ctx: &[
            field!("tab_id",   String, req, "Tab id of the affected repo."),
            field!("old_name", String, req, "Previous branch name."),
            field!("new_name", String, req, "New branch name."),
        ],
    },
    HookDef {
        name: "on_checkout",
        category: "branch",
        description: "Fired after a successful checkout. `branch` is set when checking out a named branch; `oid` is set when checking out a detached commit.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("branch", String, opt, "Branch name (when checking out a branch)."),
            field!("oid",    String, opt, "Commit oid (when checking out a detached commit)."),
        ],
    },
    HookDef {
        name: "on_tag_create",
        category: "branch",
        description: "Fired after a tag is created.",
        ctx: &[
            field!("tab_id",    String,  req, "Tab id of the affected repo."),
            field!("name",      String,  req, "Tag name."),
            field!("oid",       String,  req, "Tagged commit oid."),
            field!("annotated", Boolean, req, "True if the tag is annotated, false if lightweight."),
        ],
    },
    HookDef {
        name: "on_tag_delete",
        category: "branch",
        description: "Fired after a tag is deleted.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("name",   String, req, "Tag name."),
        ],
    },

    // ── Commit / stash / rebase ────────────────────────────────────────────
    HookDef {
        name: "on_pre_commit",
        category: "git",
        description: "Fired BEFORE a commit is created. Plugins may veto the commit by returning a non-empty string from the handler — the string is reported back to the user and the commit is aborted. Returning nil (or no value) lets the commit proceed.",
        ctx: &[
            field!("tab_id",  String,  req, "Tab id of the affected repo."),
            field!("message", String,  req, "Proposed commit message."),
            field!("amend",   Boolean, req, "True if the commit will amend HEAD."),
        ],
    },
    HookDef {
        name: "on_commit",
        category: "git",
        description: "Fired after a commit is created.",
        ctx: &[
            field!("tab_id",  String,  req, "Tab id of the affected repo."),
            field!("oid",     String,  req, "Commit oid."),
            field!("message", String,  req, "Commit message."),
            field!("amend",   Boolean, req, "True if the commit amended HEAD."),
        ],
    },
    HookDef {
        name: "on_stash_push",
        category: "git",
        description: "Fired after a stash entry is created.",
        ctx: &[
            field!("tab_id",            String,  req, "Tab id of the affected repo."),
            field!("index",             Number,  req, "Stash index (0 = newest)."),
            field!("message",           String,  req, "Stash message."),
            field!("include_untracked", Boolean, req, "Whether untracked files were stashed."),
        ],
    },
    HookDef {
        name: "on_stash_pop",
        category: "git",
        description: "Fired after a stash is cleanly applied. `drop = true` means the entry was removed (pop), `false` means it was kept (apply).",
        ctx: &[
            field!("tab_id", String,  req, "Tab id of the affected repo."),
            field!("index",  Number,  req, "Stash index that was applied."),
            field!("drop",   Boolean, req, "True if the stash entry was dropped (pop), false if kept (apply)."),
        ],
    },
    HookDef {
        name: "on_rebase_start",
        category: "git",
        description: "Fired when an interactive rebase is started.",
        ctx: &[
            field!("tab_id",       String, req, "Tab id of the affected repo."),
            field!("base",         String, req, "Base ref / oid the rebase is anchored to."),
            field!("action_count", Number, req, "Number of todo entries in the rebase plan."),
        ],
    },
    HookDef {
        name: "on_rebase_abort",
        category: "git",
        description: "Fired when an in-progress rebase is aborted.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
        ],
    },

    // ── Remote ─────────────────────────────────────────────────────────────
    HookDef {
        name: "on_fetch",
        category: "remote",
        description: "Fired after a successful fetch.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("remote", String, req, "Remote name (e.g. 'origin')."),
        ],
    },
    HookDef {
        name: "on_push",
        category: "remote",
        description: "Fired after a successful push.",
        ctx: &[
            field!("tab_id",  String,  req, "Tab id of the affected repo."),
            field!("remote",  String,  req, "Remote name."),
            field!("refspec", String,  req, "Refspec that was pushed."),
            field!("force",   Boolean, req, "True if the push was forced."),
        ],
    },
    HookDef {
        name: "on_pull",
        category: "remote",
        description: "Fired after a successful pull (fetch + fast-forward / merge).",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("remote", String, req, "Remote name."),
        ],
    },

    // ── Notes ──────────────────────────────────────────────────────────────
    HookDef {
        name: "on_note_saved",
        category: "notes",
        description: "Fired after a git note is created or updated.",
        ctx: &[
            field!("tab_id",     String, req, "Tab id of the affected repo."),
            field!("commit_oid", String, req, "Commit the note is attached to."),
            field!("namespace",  String, req, "Notes namespace (e.g. 'commits')."),
        ],
    },
    HookDef {
        name: "on_note_deleted",
        category: "notes",
        description: "Fired after a git note is deleted.",
        ctx: &[
            field!("tab_id",     String, req, "Tab id of the affected repo."),
            field!("commit_oid", String, req, "Commit the note was attached to."),
            field!("namespace",  String, req, "Notes namespace."),
        ],
    },

    // ── Git Flow ───────────────────────────────────────────────────────────
    HookDef {
        name: "on_flow_init",
        category: "gitflow",
        description: "Fired after Git Flow is initialised in a repo.",
        ctx: &[ field!("tab_id", String, req, "Tab id of the affected repo.") ],
    },
    HookDef {
        name: "on_flow_feature_start",
        category: "gitflow",
        description: "Fired after a feature branch is started.",
        ctx: &[
            field!("tab_id",      String, req, "Tab id of the affected repo."),
            field!("name",        String, req, "Feature name (without prefix)."),
            field!("base_branch", String, req, "Base branch the feature was started from."),
        ],
    },
    HookDef {
        name: "on_flow_feature_finish",
        category: "gitflow",
        description: "Fired after a feature branch is finished (merged + deleted).",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("name",   String, req, "Feature name."),
        ],
    },
    HookDef {
        name: "on_flow_release_start",
        category: "gitflow",
        description: "Fired after a release branch is started.",
        ctx: &[
            field!("tab_id",      String, req, "Tab id of the affected repo."),
            field!("version",     String, req, "Release version."),
            field!("base_branch", String, req, "Base branch the release was started from."),
        ],
    },
    HookDef {
        name: "on_flow_release_finish",
        category: "gitflow",
        description: "Fired after a release branch is finished.",
        ctx: &[
            field!("tab_id",  String, req, "Tab id of the affected repo."),
            field!("version", String, req, "Release version."),
        ],
    },
    HookDef {
        name: "on_flow_hotfix_start",
        category: "gitflow",
        description: "Fired after a hotfix branch is started.",
        ctx: &[
            field!("tab_id",      String, req, "Tab id of the affected repo."),
            field!("name",        String, req, "Hotfix name."),
            field!("base_branch", String, req, "Base branch the hotfix was started from."),
        ],
    },
    HookDef {
        name: "on_flow_hotfix_finish",
        category: "gitflow",
        description: "Fired after a hotfix branch is finished.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("name",   String, req, "Hotfix name."),
        ],
    },

    // ── Pipeline ───────────────────────────────────────────────────────────
    HookDef {
        name: "on_pipeline_started",
        category: "pipeline",
        description: "Fired when a pipeline run starts (or resumes).",
        ctx: &[
            field!("run_id",      String, req, "Run id."),
            field!("pipeline_id", String, req, "Pipeline definition id."),
            field!("plugin",      String, req, "Plugin that defined the pipeline."),
        ],
    },
    HookDef {
        name: "on_pipeline_step_done",
        category: "pipeline",
        description: "Fired when a single pipeline step finishes.",
        ctx: &[
            field!("run_id",    String, req, "Run id."),
            field!("plugin",    String, req, "Plugin that owns the pipeline."),
            field!("stage_id",  String, req, "Stage id."),
            field!("step_id",   String, req, "Step id."),
            field!("step_name", String, req, "Step display name."),
            field!("status",    String, req, "Step status: 'success' | 'failure' | 'skipped' | 'cancelled'."),
            field!("exit_code", Number, opt, "Exit code (when applicable)."),
        ],
    },
    HookDef {
        name: "on_pipeline_done",
        category: "pipeline",
        description: "Fired when a pipeline run terminates.",
        ctx: &[
            field!("run_id",      String, req, "Run id."),
            field!("pipeline_id", String, req, "Pipeline definition id."),
            field!("plugin",      String, req, "Plugin that defined the pipeline."),
            field!("status",      String, req, "Final status: 'success' | 'failure' | 'cancelled'."),
        ],
    },

    // ── Merge Request / Pull Request ───────────────────────────────────────
    HookDef {
        name: "on_mr_opened",
        category: "mr",
        description: "Fired after a merge request / pull request is opened.",
        ctx: &[
            field!("number",        Number, req, "MR / PR number."),
            field!("title",         String, req, "MR title."),
            field!("source_branch", String, req, "Source branch."),
            field!("target_branch", String, req, "Target branch."),
            field!("provider",      String, req, "Provider: 'github' | 'gitlab'."),
            field!("author",        String, req, "Author login."),
            field!("web_url",       String, req, "Provider web URL for the MR."),
        ],
    },
    HookDef {
        name: "on_mr_merged",
        category: "mr",
        description: "Fired after a merge request is merged.",
        ctx: &[
            field!("number",   Number, req, "MR number."),
            field!("provider", String, req, "Provider: 'github' | 'gitlab'."),
        ],
    },
    HookDef {
        name: "on_mr_updated",
        category: "mr",
        description: "Fired when a merge request changes state (closed, reopened, marked ready).",
        ctx: &[
            field!("number",   Number, req, "MR number."),
            field!("provider", String, req, "Provider: 'github' | 'gitlab'."),
        ],
    },

    // ── Issues (Linear / Jira) ─────────────────────────────────────────────
    HookDef {
        name: "on_issue_linked",
        category: "issues",
        description: "Fired when an issue is linked to a branch or commit.",
        ctx: &[
            field!("provider", String, req, "Provider: 'linear' | 'jira'."),
            field!("issue_id", String, req, "Provider-specific issue identifier."),
        ],
    },
    HookDef {
        name: "on_issue_transitioned",
        category: "issues",
        description: "Fired when an issue's status is changed via the Arbor UI.",
        ctx: &[
            field!("provider",   String, req, "Provider: 'linear' | 'jira'."),
            field!("issue_id",   String, req, "Provider-specific issue identifier."),
            field!("from_state", String, opt, "Previous state name (when known)."),
            field!("to_state",   String, req, "New state name."),
        ],
    },

    // ── Workspace ──────────────────────────────────────────────────────────
    HookDef {
        name: "on_workspace_created",
        category: "workspace",
        description: "Fired when a new workspace is created.",
        ctx: &[
            field!("id",         String,      req, "Workspace id."),
            field!("name",       String,      req, "Workspace name."),
            field!("color_idx",  Number,      req, "Color index."),
            field!("repo_ids",   StringArray, req, "Repo ids in the workspace."),
            field!("group_id",   String,      opt, "Parent group id (if any)."),
            field!("repo_count", Number,      req, "Number of repos."),
        ],
    },
    HookDef {
        name: "on_workspace_updated",
        category: "workspace",
        description: "Fired when a workspace's metadata is updated (name, color, group).",
        ctx: &[
            field!("id",         String,      req, "Workspace id."),
            field!("name",       String,      req, "Workspace name."),
            field!("color_idx",  Number,      req, "Color index."),
            field!("repo_ids",   StringArray, req, "Repo ids in the workspace."),
            field!("group_id",   String,      opt, "Parent group id."),
            field!("repo_count", Number,      req, "Number of repos."),
        ],
    },
    HookDef {
        name: "on_workspace_deleted",
        category: "workspace",
        description: "Fired when a workspace is deleted.",
        ctx: &[
            field!("id",   String, req, "Workspace id."),
            field!("name", String, req, "Workspace name."),
        ],
    },
    HookDef {
        name: "on_workspace_switched",
        category: "workspace",
        description: "Fired when the active workspace changes.",
        ctx: &[
            field!("id",         String,      req, "Workspace id."),
            field!("name",       String,      req, "Workspace name."),
            field!("color_idx",  Number,      req, "Color index."),
            field!("repo_ids",   StringArray, req, "Repo ids in the workspace."),
            field!("group_id",   String,      opt, "Parent group id."),
            field!("repo_count", Number,      req, "Number of repos."),
        ],
    },
    HookDef {
        name: "on_workspace_repo_added",
        category: "workspace",
        description: "Fired when a repo is added to a workspace.",
        ctx: &[
            field!("workspace_id", String, req, "Workspace id."),
            field!("repo_id",      String, req, "Repo id."),
        ],
    },
    HookDef {
        name: "on_workspace_repo_removed",
        category: "workspace",
        description: "Fired when a repo is removed from a workspace.",
        ctx: &[
            field!("workspace_id", String, req, "Workspace id."),
            field!("repo_id",      String, req, "Repo id."),
        ],
    },

    // ── Theme / branding ───────────────────────────────────────────────────
    HookDef {
        name: "on_theme_changed",
        category: "theme",
        description: "Fired when the active theme changes — either the user picks a different theme, the app boots and applies the persisted choice, or a plugin overlays / clears extra CSS tokens. The `vars` payload carries the merged effective stylesheet (active theme + every plugin overlay).",
        ctx: &[
            field!("theme_id",   String, req, "Active theme id (e.g. 'dark', 'custom-acme-…')."),
            field!("theme_name", String, req, "Active theme display name."),
            field!("vars",       Object, req, "Merged map of `--css-var` → value currently in force."),
            field!("source",     String, req, "What triggered the change: 'user' | 'plugin' | 'init'."),
        ],
    },

    // ── Security dashboard ─────────────────────────────────────────────────
    HookDef {
        name: "on_security_summary_loaded",
        category: "security",
        description: "Fired after the security dashboard summary is fetched for a tab. The counts in `ctx` are active-only (Detected + Confirmed) — closed findings are excluded just like in the panel itself.",
        ctx: &[
            field!("tab_id",     String, req, "Tab id of the affected repo."),
            field!("provider",   String, req, "Provider kind: 'github' | 'gitlab'."),
            field!("counts",     Object, req, "Severity counts map: { critical, high, medium, low, info, unknown }."),
            field!("total",      Number, req, "Total active findings across all severities."),
            field!("risk_label", String, opt, "Risk-score band ('Low' | 'Medium' | 'High' | 'Critical') when available."),
            field!("web_url",    String, opt, "Provider-native dashboard URL."),
        ],
    },
    HookDef {
        name: "on_security_finding_state_changed",
        category: "security",
        description: "Fired by `arbor.security.*` consumers (or the host on rescan) when a finding moves between active and closed states. Use it to drive notifications or external trackers; the host itself does not emit this on every fetch — it's a plugin-cooperation channel keyed off finding ids the plugin observes.",
        ctx: &[
            field!("tab_id",      String, req, "Tab id of the affected repo."),
            field!("finding_id",  String, req, "Provider-stable finding id."),
            field!("severity",    String, req, "Severity: 'critical' | 'high' | 'medium' | 'low' | 'info' | 'unknown'."),
            field!("from_state",  String, opt, "Previous state (when known)."),
            field!("to_state",    String, req, "New state: 'detected' | 'confirmed' | 'resolved' | 'dismissed'."),
            field!("title",       String, opt, "Finding title."),
            field!("web_url",     String, opt, "Provider URL for the finding."),
        ],
    },

    // ── Linked Worktrees (cross-project sync) ──────────────────────────────
    HookDef {
        name: "on_worktree_link_sync_started",
        category: "linked_worktrees",
        description: "Fired when a cross-project branch sync starts.",
        ctx: &[
            field!("link_id",           String, req, "Linked-worktree id."),
            field!("link_name",         String, req, "Linked-worktree display name."),
            field!("initiator_repo_id", String, req, "Repo that triggered the sync."),
            field!("target_branch",     String, req, "Branch the initiator just checked out."),
        ],
    },
    HookDef {
        name: "on_worktree_link_sync_done",
        category: "linked_worktrees",
        description: "Fired when a cross-project branch sync finishes. Payload contains a per-member outcome summary.",
        ctx: &[
            field!("link_id",           String, req, "Linked-worktree id."),
            field!("link_name",         String, req, "Linked-worktree display name."),
            field!("initiator_repo_id", String, req, "Repo that triggered the sync."),
            field!("target_branch",     String, req, "Synced branch."),
            field!("results",           Object, req, "Map of repo_id → outcome { status, message? }."),
        ],
    },
    HookDef {
        name: "on_worktree_link_member_added",
        category: "linked_worktrees",
        description: "Fired when a repo is added to a linked-worktree group.",
        ctx: &[
            field!("link_id", String, req, "Linked-worktree id."),
            field!("repo_id", String, req, "Repo id added to the group."),
        ],
    },
    HookDef {
        name: "on_worktree_link_member_removed",
        category: "linked_worktrees",
        description: "Fired when a repo is removed from a linked-worktree group.",
        ctx: &[
            field!("link_id", String, req, "Linked-worktree id."),
            field!("repo_id", String, req, "Repo id removed from the group."),
        ],
    },
];

/// Look up a hook by name. Returns None for unknown hooks (action hooks
/// fired via `arbor.events.emit` or `arbor.command.register` are not in
/// the catalog — they're plugin-defined).
pub fn find(name: &str) -> Option<&'static HookDef> {
    HOOK_CATALOG.iter().find(|h| h.name == name)
}
