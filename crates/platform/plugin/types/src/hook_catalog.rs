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
//!
//! ## Names come from [`crate::hook_names`], never from a literal
//!
//! Every `name:` below is a constant, and the fire sites use the *same*
//! constant (**D10**). That is the only reason the catalog can be trusted as
//! the answer to "does this hook exist": a catalog entry and a fire site
//! cannot describe two different strings.
//!
//! ## The catalog is also the subscribe-time authority
//!
//! [`resolve_subscription`] turns what a plugin wrote into the name it will
//! actually receive, and [`find`] decides whether that name is real. Together
//! they close the last hole in the naming story: on the Lua side there is no
//! compile step, so `arbor.events.on("note_savd", …)` used to resolve to a
//! plausible-looking name that nothing ever fires, and the plugin simply did
//! nothing with no message at all.

use std::borrow::Cow;

use crate::hook_names::{arbor, corvus, garrulus, pipeline};
use crate::hook_ns;

#[derive(Copy, Clone, Debug)]
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

#[derive(Copy, Clone, Debug)]
pub struct HookField {
    pub name:        &'static str,
    pub ty:          FieldType,
    pub required:    bool,
    pub description: &'static str,
}

#[derive(Copy, Clone, Debug)]
pub struct HookDef {
    pub name:        &'static str,
    pub category:    &'static str,
    pub description: &'static str,
    pub ctx:         &'static [HookField],
}

impl HookDef {
    /// The namespace half of [`HookDef::name`].
    ///
    /// Never `None` for a catalog entry — every built-in name is qualified —
    /// but typed as an `Option` because the splitter is shared with
    /// user-supplied strings.
    pub fn namespace(&self) -> Option<&'static str> {
        hook_ns::namespace_of(self.name)
    }

    /// The event half of [`HookDef::name`], without the namespace.
    pub fn event(&self) -> &'static str {
        hook_ns::event_of(self.name)
    }
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
    // ── Lifecycle (host runtime) ───────────────────────────────────────────
    HookDef {
        name: arbor::PLUGIN_LOAD,
        category: "lifecycle",
        description: "Fired once after the plugin's main.lua finishes executing. Use it as the plugin constructor.",
        ctx: NO_CTX,
    },
    HookDef {
        name: arbor::PLUGIN_UNLOAD,
        category: "lifecycle",
        description: "Fired before the plugin is unloaded (reload, disable, app shutdown). Use it to release resources.",
        ctx: NO_CTX,
    },

    // ── Project lifecycle (host runtime) ───────────────────────────────────
    // In the host namespace, not a product's: the shared backend plumbing every
    // product links fires these, and a plugin loaded under two products must see
    // the same hook from the same source line.
    HookDef {
        name: arbor::REPO_OPEN,
        category: "repo",
        description: "Fired when the user opens a project (new tab or after a plugin reload).",
        ctx: TAB_PATH_NAME_CTX,
    },
    HookDef {
        name: arbor::REPO_CLOSE,
        category: "repo",
        description: "Fired when the user closes a project tab.",
        ctx: TAB_PATH_NAME_CTX,
    },
    HookDef {
        name: arbor::TAB_SWITCH,
        category: "repo",
        description: "Fired when the user activates a different project tab.",
        ctx: TAB_PATH_NAME_CTX,
    },

    // ── Main-area views (host runtime) ─────────────────────────────────────
    HookDef {
        name: arbor::VIEW_OPEN,
        category: "view",
        description: "Fired on the owning plugin when one of its main-area views (registered via `arbor.ui.add_view`) is opened. Respond by pushing the body with `arbor.ui.set_panel_content(view_id, …)`. Targeted at the owner only — not a broadcast.",
        ctx: &[
            field!("view_id", String, req, "Id of the view that was opened."),
            field!("label",   String, opt, "Display label of the view."),
        ],
    },
    HookDef {
        name: arbor::VIEW_CLOSE,
        category: "view",
        description: "Fired on the owning plugin when one of its main-area views is closed (toggled off, replaced by another view, or the plugin reloaded). Use it to release per-view resources or stop polling.",
        ctx: &[
            field!("view_id", String, req, "Id of the view that was closed."),
            field!("label",   String, opt, "Display label of the view."),
        ],
    },

    // ── Theme / branding (host runtime) ────────────────────────────────────
    HookDef {
        name: arbor::THEME_CHANGED,
        category: "theme",
        description: "Fired when the active theme changes — either the user picks a different theme, the app boots and applies the persisted choice, or a plugin overlays / clears extra CSS tokens. The `vars` payload carries the merged effective stylesheet (active theme + every plugin overlay).",
        ctx: &[
            field!("theme_id",   String, req, "Active theme id (e.g. 'dark', 'custom-acme-…')."),
            field!("theme_name", String, req, "Active theme display name."),
            field!("vars",       Object, req, "Merged map of `--css-var` → value currently in force."),
            field!("source",     String, req, "What triggered the change: 'user' | 'plugin' | 'init'."),
        ],
    },

    // ── Repo registry (corvus) ─────────────────────────────────────────────
    HookDef {
        name: corvus::REPO_INIT,
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
        name: corvus::REPO_DEREGISTERED,
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
        name: corvus::PROJECT_MISSING,
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
        name: corvus::PROJECT_RELOCATED,
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

    // ── Branch / tag ───────────────────────────────────────────────────────
    HookDef {
        name: corvus::BRANCH_CREATE,
        category: "branch",
        description: "Fired after a new local branch is created.",
        ctx: &[
            field!("tab_id",   String, req, "Tab id of the affected repo."),
            field!("name",     String, req, "Branch name."),
            field!("from_oid", String, req, "Commit oid the branch was created from."),
        ],
    },
    HookDef {
        name: corvus::BRANCH_DELETE,
        category: "branch",
        description: "Fired after one or more local branches are deleted. Single-branch deletes carry `name`; bulk deletes carry `names`.",
        ctx: &[
            field!("tab_id", String,      req, "Tab id of the affected repo."),
            field!("name",   String,      opt, "Branch name (single-delete variant)."),
            field!("names",  StringArray, opt, "Branch names (bulk-delete variant)."),
        ],
    },
    HookDef {
        name: corvus::BRANCH_RENAME,
        category: "branch",
        description: "Fired after a local branch is renamed.",
        ctx: &[
            field!("tab_id",   String, req, "Tab id of the affected repo."),
            field!("old_name", String, req, "Previous branch name."),
            field!("new_name", String, req, "New branch name."),
        ],
    },
    HookDef {
        name: corvus::CHECKOUT,
        category: "branch",
        description: "Fired after a successful checkout. `branch` is set when checking out a named branch; `oid` is set when checking out a detached commit.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("branch", String, opt, "Branch name (when checking out a branch)."),
            field!("oid",    String, opt, "Commit oid (when checking out a detached commit)."),
        ],
    },
    HookDef {
        name: corvus::TAG_CREATE,
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
        name: corvus::TAG_DELETE,
        category: "branch",
        description: "Fired after a tag is deleted.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("name",   String, req, "Tag name."),
        ],
    },

    // ── Commit / stash / rebase ────────────────────────────────────────────
    HookDef {
        name: corvus::PRE_COMMIT,
        category: "git",
        description: "Fired BEFORE a commit is created. Plugins may veto the commit by returning a non-empty string from the handler — the string is reported back to the user and the commit is aborted. Returning nil (or no value) lets the commit proceed.",
        ctx: &[
            field!("tab_id",  String,  req, "Tab id of the affected repo."),
            field!("message", String,  req, "Proposed commit message."),
            field!("amend",   Boolean, req, "True if the commit will amend HEAD."),
        ],
    },
    HookDef {
        name: corvus::COMMIT,
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
        name: corvus::STASH_PUSH,
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
        name: corvus::STASH_POP,
        category: "git",
        description: "Fired after a stash is cleanly applied. `drop = true` means the entry was removed (pop), `false` means it was kept (apply).",
        ctx: &[
            field!("tab_id", String,  req, "Tab id of the affected repo."),
            field!("index",  Number,  req, "Stash index that was applied."),
            field!("drop",   Boolean, req, "True if the stash entry was dropped (pop), false if kept (apply)."),
        ],
    },
    HookDef {
        name: corvus::REBASE_START,
        category: "git",
        description: "Fired when an interactive rebase is started.",
        ctx: &[
            field!("tab_id",       String, req, "Tab id of the affected repo."),
            field!("base",         String, req, "Base ref / oid the rebase is anchored to."),
            field!("action_count", Number, req, "Number of todo entries in the rebase plan."),
        ],
    },
    HookDef {
        name: corvus::REBASE_ABORT,
        category: "git",
        description: "Fired when an in-progress rebase is aborted.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
        ],
    },

    // ── Remote ─────────────────────────────────────────────────────────────
    HookDef {
        name: corvus::FETCH,
        category: "remote",
        description: "Fired after a successful fetch.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("remote", String, req, "Remote name (e.g. 'origin')."),
        ],
    },
    HookDef {
        name: corvus::PUSH,
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
        name: corvus::PULL,
        category: "remote",
        description: "Fired after a successful pull (fetch + fast-forward / merge).",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("remote", String, req, "Remote name."),
        ],
    },

    // ── Git notes ──────────────────────────────────────────────────────────
    HookDef {
        name: corvus::NOTE_SAVED,
        category: "notes",
        description: "Fired after a git note is created or updated. The vault's equivalent is `garrulus:note_saved`, which carries a completely different payload.",
        ctx: &[
            field!("tab_id",     String, req, "Tab id of the affected repo."),
            field!("commit_oid", String, req, "Commit the note is attached to."),
            field!("namespace",  String, req, "Notes namespace (e.g. 'commits')."),
        ],
    },
    HookDef {
        name: corvus::NOTE_DELETED,
        category: "notes",
        description: "Fired after a git note is deleted. The vault's equivalent is `garrulus:note_deleted`.",
        ctx: &[
            field!("tab_id",     String, req, "Tab id of the affected repo."),
            field!("commit_oid", String, req, "Commit the note was attached to."),
            field!("namespace",  String, req, "Notes namespace."),
        ],
    },

    // ── Git Flow ───────────────────────────────────────────────────────────
    HookDef {
        name: corvus::FLOW_INIT,
        category: "gitflow",
        description: "Fired after Git Flow is initialised in a repo.",
        ctx: &[ field!("tab_id", String, req, "Tab id of the affected repo.") ],
    },
    HookDef {
        name: corvus::FLOW_FEATURE_START,
        category: "gitflow",
        description: "Fired after a feature branch is started.",
        ctx: &[
            field!("tab_id",      String, req, "Tab id of the affected repo."),
            field!("name",        String, req, "Feature name (without prefix)."),
            field!("base_branch", String, req, "Base branch the feature was started from."),
        ],
    },
    HookDef {
        name: corvus::FLOW_FEATURE_FINISH,
        category: "gitflow",
        description: "Fired after a feature branch is finished (merged + deleted).",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("name",   String, req, "Feature name."),
        ],
    },
    HookDef {
        name: corvus::FLOW_RELEASE_START,
        category: "gitflow",
        description: "Fired after a release branch is started.",
        ctx: &[
            field!("tab_id",      String, req, "Tab id of the affected repo."),
            field!("version",     String, req, "Release version."),
            field!("base_branch", String, req, "Base branch the release was started from."),
        ],
    },
    HookDef {
        name: corvus::FLOW_RELEASE_FINISH,
        category: "gitflow",
        description: "Fired after a release branch is finished.",
        ctx: &[
            field!("tab_id",  String, req, "Tab id of the affected repo."),
            field!("version", String, req, "Release version."),
        ],
    },
    HookDef {
        name: corvus::FLOW_HOTFIX_START,
        category: "gitflow",
        description: "Fired after a hotfix branch is started.",
        ctx: &[
            field!("tab_id",      String, req, "Tab id of the affected repo."),
            field!("name",        String, req, "Hotfix name."),
            field!("base_branch", String, req, "Base branch the hotfix was started from."),
        ],
    },
    HookDef {
        name: corvus::FLOW_HOTFIX_FINISH,
        category: "gitflow",
        description: "Fired after a hotfix branch is finished.",
        ctx: &[
            field!("tab_id", String, req, "Tab id of the affected repo."),
            field!("name",   String, req, "Hotfix name."),
        ],
    },

    // ── Merge Request / Pull Request ───────────────────────────────────────
    HookDef {
        name: corvus::MR_OPENED,
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
        name: corvus::MR_MERGED,
        category: "mr",
        description: "Fired after a merge request is merged.",
        ctx: &[
            field!("number",   Number, req, "MR number."),
            field!("provider", String, req, "Provider: 'github' | 'gitlab'."),
        ],
    },
    HookDef {
        name: corvus::MR_UPDATED,
        category: "mr",
        description: "Fired when a merge request changes state (closed, reopened, marked ready).",
        ctx: &[
            field!("number",   Number, req, "MR number."),
            field!("provider", String, req, "Provider: 'github' | 'gitlab'."),
        ],
    },

    // ── Issues (Linear / Jira) ─────────────────────────────────────────────
    HookDef {
        name: corvus::ISSUE_LINKED,
        category: "issues",
        description: "Fired when an issue is linked to a branch or commit.",
        ctx: &[
            field!("provider", String, req, "Provider: 'linear' | 'jira'."),
            field!("issue_id", String, req, "Provider-specific issue identifier."),
        ],
    },
    HookDef {
        name: corvus::ISSUE_TRANSITIONED,
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
        name: corvus::WORKSPACE_CREATED,
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
        name: corvus::WORKSPACE_UPDATED,
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
        name: corvus::WORKSPACE_DELETED,
        category: "workspace",
        description: "Fired when a workspace is deleted.",
        ctx: &[
            field!("id",   String, req, "Workspace id."),
            field!("name", String, req, "Workspace name."),
        ],
    },
    HookDef {
        name: corvus::WORKSPACE_SWITCHED,
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
        name: corvus::WORKSPACE_REPO_ADDED,
        category: "workspace",
        description: "Fired when a repo is added to a workspace.",
        ctx: &[
            field!("workspace_id", String, req, "Workspace id."),
            field!("repo_id",      String, req, "Repo id."),
        ],
    },
    HookDef {
        name: corvus::WORKSPACE_REPO_REMOVED,
        category: "workspace",
        description: "Fired when a repo is removed from a workspace.",
        ctx: &[
            field!("workspace_id", String, req, "Workspace id."),
            field!("repo_id",      String, req, "Repo id."),
        ],
    },

    // ── Security dashboard ─────────────────────────────────────────────────
    HookDef {
        name: corvus::SECURITY_SUMMARY_LOADED,
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
        name: corvus::SECURITY_FINDING_STATE_CHANGED,
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
        name: corvus::WORKTREE_LINK_SYNC_STARTED,
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
        name: corvus::WORKTREE_LINK_SYNC_DONE,
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
        name: corvus::WORKTREE_LINK_MEMBER_ADDED,
        category: "linked_worktrees",
        description: "Fired when a repo is added to a linked-worktree group.",
        ctx: &[
            field!("link_id", String, req, "Linked-worktree id."),
            field!("repo_id", String, req, "Repo id added to the group."),
        ],
    },
    HookDef {
        name: corvus::WORKTREE_LINK_MEMBER_REMOVED,
        category: "linked_worktrees",
        description: "Fired when a repo is removed from a linked-worktree group.",
        ctx: &[
            field!("link_id", String, req, "Linked-worktree id."),
            field!("repo_id", String, req, "Repo id removed from the group."),
        ],
    },

    // ── Garrulus vault ─────────────────────────────────────────────────────
    // Paths in these categories are always vault-relative with POSIX
    // separators, except the vault root itself.
    HookDef {
        name: garrulus::VAULT_OPENED,
        category: "vault",
        description: "Fired after a Garrulus note vault is opened or created and its index is built.",
        ctx: &[
            field!("vault_id",   String, req, "Stable vault id (also the key of its index cache)."),
            field!("path",       String, req, "Absolute vault root on disk."),
            field!("name",       String, req, "Display name shown in the vault switcher."),
            field!("note_count", Number, req, "Notes indexed at open."),
        ],
    },
    HookDef {
        name: garrulus::VAULT_CLOSED,
        category: "vault",
        description: "Fired after the open vault is closed (watcher stopped, index emptied, remote detached). Not fired when no vault was open.",
        ctx: &[
            field!("path", String, req, "Absolute root of the vault that closed."),
        ],
    },
    HookDef {
        name: garrulus::TYPE_APPLIED,
        category: "vault",
        description: "Fired after a note is tagged as being of a note type (its frontmatter `type` key is set). Fires even when the note already carried that type and nothing was rewritten.",
        ctx: &[
            field!("path", String, req, "Vault-relative path of the note."),
            field!("type", String, req, "Note type id that was applied."),
        ],
    },

    // ── Garrulus vault notes ───────────────────────────────────────────────
    // Distinct from the `notes` category above, which is git notes: these carry
    // a vault-relative `path`, never a `tab_id` / `commit_oid`. The namespace is
    // what keeps them apart — the payloads are unrelated.
    HookDef {
        name: garrulus::NOTE_CREATED,
        category: "vault_notes",
        description: "Fired after a note that did not exist is written into the vault — a new note, or one restored from the vault trash.",
        ctx: &[
            field!("path",   String, req, "Vault-relative path of the new note."),
            field!("source", String, opt, "'trash' when the note came back from the vault trash; absent for a freshly created note."),
        ],
    },
    HookDef {
        name: garrulus::NOTE_SAVED,
        category: "vault_notes",
        description: "Fired after a vault note's text is written to disk and re-indexed.",
        ctx: &[
            field!("path",   String, req, "Vault-relative path of the saved note."),
            field!("bytes",  Number, opt, "Length in bytes of the text written (ordinary save only)."),
            field!("source", String, opt, "'conflict' when the remote side of a sync conflict was adopted as the note; absent on an ordinary save."),
        ],
    },
    HookDef {
        name: garrulus::NOTE_RENAMED,
        category: "vault_notes",
        description: "Fired after a vault note is moved to a new path. Wikilinks pointing at it are NOT rewritten by this operation — the rename-with-link-update flow performs that rewrite as ordinary saves before calling this.",
        ctx: &[
            field!("old_path", String, req, "Vault-relative path the note had."),
            field!("new_path", String, req, "Vault-relative path the note now has."),
        ],
    },
    HookDef {
        name: garrulus::NOTE_DELETED,
        category: "vault_notes",
        description: "Fired after a vault note is moved into the vault's trash. Not a hard delete: the note can be put back with its trash id.",
        ctx: &[
            field!("path",     String, req, "Vault-relative path the note had."),
            field!("trash_id", String, req, "Id of the trash entry, for a later restore."),
        ],
    },

    // ── Garrulus sync ──────────────────────────────────────────────────────
    // Only ever fired from a handler a user's click reached: the background probe
    // is read-only and fires nothing.
    HookDef {
        name: garrulus::SYNC_STARTED,
        category: "vault_sync",
        description: "Fired when a vault sync operation begins. Never fired by the background probe — every sync is a user action.",
        ctx: &[
            field!("op",    String, req, "Operation: 'pull' | 'push' | 'sync' (pull then push)."),
            field!("notes", Number, opt, "Notes in an explicit push batch; 0 means 'everything changed'. Push only."),
        ],
    },
    HookDef {
        name: garrulus::SYNC_DONE,
        category: "vault_sync",
        description: "Fired when a vault sync operation finishes successfully. A failed operation returns an error to the caller and fires nothing.",
        ctx: &[
            field!("op",        String, req, "Operation that finished: 'pull' | 'push' | 'sync'."),
            field!("applied",   Number, opt, "Notes the pull brought in (pull and sync only)."),
            field!("conflicts", Number, opt, "Conflicts the pull could not merge (pull and sync only). A non-zero count means the push half of a 'sync' was skipped."),
        ],
    },
    HookDef {
        name: garrulus::SYNC_CONFLICT,
        category: "vault_sync",
        description: "Fired after a pull that produced conflicts, before the matching `garrulus:sync_done`. No merge marker is ever written into a note: each remote side is written as its own file beside it, and the user resolves from the Conflicts panel.",
        ctx: &[
            field!("count", Number, req, "Number of conflicted notes."),
        ],
    },

    // ── Pipeline ───────────────────────────────────────────────────────────
    HookDef {
        name: pipeline::STARTED,
        category: "pipeline",
        description: "Fired when a pipeline run starts (or resumes).",
        ctx: &[
            field!("run_id",      String, req, "Run id."),
            field!("pipeline_id", String, req, "Pipeline definition id."),
            field!("plugin",      String, req, "Plugin that defined the pipeline."),
        ],
    },
    HookDef {
        name: pipeline::STEP_DONE,
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
        name: pipeline::DONE,
        category: "pipeline",
        description: "Fired when a pipeline run terminates.",
        ctx: &[
            field!("run_id",      String, req, "Run id."),
            field!("pipeline_id", String, req, "Pipeline definition id."),
            field!("plugin",      String, req, "Plugin that defined the pipeline."),
            field!("status",      String, req, "Final status: 'success' | 'failure' | 'cancelled'."),
        ],
    },
    HookDef {
        name: pipeline::RUN_REQUEST,
        category: "pipeline",
        description: "Delivered to a single plugin when the user launches a pipeline the plugin declared without stages — the plugin is expected to compile and run it. Targeted, never broadcast: a plugin that declares such a pipeline and does not subscribe gets a launch error instead.",
        ctx: &[
            field!("pipeline_id", String, req, "Pipeline definition id to execute."),
            field!("tab_id",      String, req, "Tab id the launch came from."),
        ],
    },
];

/// Look up a hook by its fully-qualified name.
///
/// Returns `None` for anything the host does not fire: plugin-defined events
/// (`arbor.events.emit`), command / timer / job callbacks, and targeted
/// delivery names are all legal and all absent from the catalog. Callers must
/// therefore treat `None` as "not a built-in", not as "invalid".
pub fn find(name: &str) -> Option<&'static HookDef> {
    HOOK_CATALOG.iter().find(|h| h.name == name)
}

/// Every catalog entry belonging to `ns`.
///
/// Lets a backend register only the hooks it can actually fire, so
/// `arbor.hooks.list()` in one product stops advertising another product's
/// hooks as if they were reachable.
pub fn hooks_in_ns(ns: &str) -> impl Iterator<Item = &'static HookDef> + '_ {
    HOOK_CATALOG.iter().filter(move |h| h.namespace() == Some(ns))
}

/// True when `ns` is a namespace the host fires hooks in.
///
/// The discriminator between "this plugin mistyped a built-in" and "this plugin
/// subscribed to another plugin's event": `my-plugin:build_done` is a perfectly
/// good name that will never be in the catalog, while `corvus:commmit` is a
/// typo worth a warning.
pub fn is_known_namespace(ns: &str) -> bool {
    HOOK_CATALOG.iter().any(|h| h.namespace() == Some(ns))
}

/// Resolve what a plugin wrote in `arbor.events.on(...)` into the name it will
/// actually be subscribed under (**D9**).
///
/// The rules, in order:
///
/// 1. A pattern (`*`, `garrulus:*`) is left exactly as written — rewriting it
///    would narrow "everything" to "everything from one product".
/// 2. An already-qualified name is left as written, including a namespace that
///    is not the host's: subscribing to another product's hook is legal and is
///    how a cross-product plugin is written.
/// 3. An unqualified name is prefixed with the host product's id — the same
///    optional-prefix rule `arbor.events.emit` applies with the plugin's own
///    name.
/// 4. …unless that product-qualified name is not a real hook and the host
///    namespace has one by that event. Lifecycle hooks (`plugin_load`,
///    `view_open`, `theme_changed`, …) belong to no product, and the same
///    `main.lua` line loaded under two hosts has to mean the same thing.
pub fn resolve_subscription<'a>(raw: &'a str, product: &str) -> Cow<'a, str> {
    if hook_ns::is_pattern(raw) || hook_ns::split_ns(raw).is_some() {
        return Cow::Borrowed(raw);
    }

    let qualified = format!("{product}{}{raw}", hook_ns::HOOK_NS_SEP);
    if find(&qualified).is_some() {
        return Cow::Owned(qualified);
    }

    let host_qualified = format!("{}{}{raw}", arbor::NS, hook_ns::HOOK_NS_SEP);
    if find(&host_qualified).is_some() {
        return Cow::Owned(host_qualified);
    }

    // Neither exists: keep the product-qualified form so the warning names what
    // the plugin will actually be waiting for, not a guess.
    Cow::Owned(qualified)
}

/// The catalog entries closest to `name`, for a "did you mean" message.
pub fn nearest(name: &str, max: usize) -> Vec<&'static str> {
    hook_ns::nearest_names(name, HOOK_CATALOG.iter().map(|h| h.name), max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_names::NAMESPACES;
    use std::collections::HashSet;

    #[test]
    fn every_declared_name_has_a_catalog_entry() {
        for (_, names) in NAMESPACES {
            for &name in *names {
                assert!(find(name).is_some(), "'{name}' is declared but not in the catalog");
            }
        }
    }

    #[test]
    fn every_catalog_entry_is_a_declared_name() {
        let declared: HashSet<&str> =
            NAMESPACES.iter().flat_map(|(_, names)| names.iter().copied()).collect();
        for h in HOOK_CATALOG {
            assert!(declared.contains(h.name), "'{}' is in the catalog but not declared", h.name);
        }
    }

    #[test]
    fn catalog_names_are_unique() {
        let mut seen: HashSet<&str> = HashSet::new();
        for h in HOOK_CATALOG {
            assert!(seen.insert(h.name), "duplicate catalog entry '{}'", h.name);
        }
    }

    #[test]
    fn hooks_in_ns_partitions_the_catalog() {
        let total: usize = NAMESPACES.iter().map(|&(ns, _)| hooks_in_ns(ns).count()).sum();
        assert_eq!(total, HOOK_CATALOG.len());
    }

    #[test]
    fn unqualified_name_takes_the_host_product() {
        assert_eq!(resolve_subscription("commit", "corvus"), "corvus:commit");
        assert_eq!(resolve_subscription("note_saved", "garrulus"), "garrulus:note_saved");
    }

    /// The failure mode the product-prefix rule alone would create: one source
    /// line, two hosts, two different hooks.
    #[test]
    fn lifecycle_names_fall_back_to_the_host_namespace() {
        assert_eq!(resolve_subscription("plugin_load", "corvus"), "arbor:plugin_load");
        assert_eq!(resolve_subscription("plugin_load", "garrulus"), "arbor:plugin_load");
        assert_eq!(resolve_subscription("view_open", "sitta"), "arbor:view_open");
    }

    #[test]
    fn a_qualified_name_is_never_rewritten() {
        assert_eq!(resolve_subscription("garrulus:note_saved", "corvus"), "garrulus:note_saved");
        assert_eq!(resolve_subscription("arbor:plugin_load", "corvus"), "arbor:plugin_load");
    }

    #[test]
    fn patterns_survive_resolution_untouched() {
        assert_eq!(resolve_subscription("*", "corvus"), "*");
        assert_eq!(resolve_subscription("garrulus:*", "corvus"), "garrulus:*");
        assert_eq!(resolve_subscription("*_saved", "corvus"), "*_saved");
    }

    #[test]
    fn an_unknown_name_keeps_the_product_prefix() {
        // Nothing to guess at: the message downstream has to say what the
        // plugin is actually waiting for.
        assert_eq!(resolve_subscription("note_savd", "garrulus"), "garrulus:note_savd");
    }

    #[test]
    fn nearest_finds_the_typo() {
        let got = nearest("garrulus:note_savd", 3);
        assert!(got.contains(&garrulus::NOTE_SAVED), "got {got:?}");
    }

    #[test]
    fn known_namespaces_are_exactly_the_declared_ones() {
        for &(ns, _) in NAMESPACES {
            assert!(is_known_namespace(ns), "'{ns}' fires hooks but is not recognised");
        }
        assert!(!is_known_namespace("my-plugin"));
    }

    #[test]
    fn namespace_and_event_split_out_of_a_catalog_entry() {
        let def = find(corvus::PRE_COMMIT).expect("pre_commit is in the catalog");
        assert_eq!(def.namespace(), Some("corvus"));
        assert_eq!(def.event(), "pre_commit");
    }
}
