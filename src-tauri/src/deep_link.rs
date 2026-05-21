//! Deep-link (`arbor://…`) plumbing — the actual URL routing happens in
//! the frontend; this module owns:
//!
//!   * configuration knobs (cross-workspace strategy, …)
//!   * the cold-start URL buffer that holds links arriving before the
//!     webview has had a chance to register its event listener
//!
//! See `commands/deep_link_commands.rs` for the IPC surface and `lib.rs`
//! for the wiring of the deep-link plugin's `on_open_url` callback.

use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Persisted config — written to `~/.config/arbor/config.toml` under
// `[deep_link]`. Serialised on every save via the host AppConfig.
// ---------------------------------------------------------------------------

/// What to do when an incoming deep-link points to a repo whose registry
/// entry is a member of one or more workspaces *other* than the active one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossWorkspaceStrategy {
    /// Switch the active workspace to the first match (in user-defined order)
    /// and activate the existing tab inside it.
    Switch,
    /// Open the repo as a tab in the *current* workspace, marked
    /// cross-workspace.  Doesn't disturb the user's current focus.
    OpenHere,
}

impl Default for CrossWorkspaceStrategy {
    fn default() -> Self { Self::Switch }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepLinkConfig {
    /// Master kill-switch.  When false, every incoming `arbor://…` URL
    /// short-circuits to a "feature disabled" modal — Arbor never touches
    /// repos, never opens modals, never runs workflows.  Default: **false**
    /// (CYA — users opt in explicitly via Settings).
    #[serde(default)]
    pub enabled: bool,
    /// Per-action enable toggles.  All default to **false**: even after
    /// flipping the master `enabled`, the user must individually opt in
    /// each action kind they want active.  This is intentional belt-and-
    /// braces — sharing a link should never silently mutate a workspace.
    #[serde(default)]
    pub enable: EnableConfig,
    #[serde(default)]
    pub cross_workspace_strategy: CrossWorkspaceStrategy,
    /// Per-action confirm prompts.  Default is "ask before doing anything"
    /// — every kind starts at `true` — but power users can disable
    /// individual entries for actions they trust (e.g. `commit_jump`).
    /// The clone-confirm modal is **independent** of this section: a missing
    /// local copy ALWAYS asks for consent regardless of these toggles.
    #[serde(default)]
    pub confirm: ConfirmConfig,
    /// When true, an incoming `arbor://branch/<name>?checkout=1` is silently
    /// rewritten to the worktree variant before dispatch.  Useful when your
    /// workflow is "every shared branch becomes a new worktree" — avoids
    /// accidentally moving HEAD on the main checkout when colleagues share
    /// a branch link.  Default: false (preserve the link's literal intent).
    #[serde(default)]
    pub checkout_as_worktree: bool,
    /// Host (domain, no scheme, no trailing slash) of the HTTPS redirect
    /// worker used when generating shareable deep links.  Chats like Google
    /// Chat / Slack / Teams don't render `arbor://…` URLs as clickable, so
    /// Arbor instead emits `https://<worker_base_url>/<path>?<query>` and
    /// relies on a tiny Cloudflare Worker (or equivalent) to 302-redirect
    /// the request back to the equivalent `arbor://` URL.  Leave empty to
    /// fall back to the raw `arbor://` scheme when copying links.
    /// Default: the project's hosted worker.
    #[serde(default = "default_worker_base_url")]
    pub worker_base_url: String,
}

fn default_worker_base_url() -> String {
    "arbor-redirect.nightprint-studio.workers.dev".into()
}

/// Per-deep-link-action enable toggles.  All default to `false`: the
/// dispatcher refuses an action whose flag is off and shows the
/// "Deep link disabled" modal explaining which kind was blocked.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnableConfig {
    #[serde(default)]
    pub repo_open:       bool,
    #[serde(default)]
    pub commit_jump:     bool,
    #[serde(default)]
    pub branch_checkout: bool,
    #[serde(default)]
    pub branch_worktree: bool,
    #[serde(default)]
    pub mr_open:         bool,
    #[serde(default)]
    pub pipeline_open:   bool,
}

/// Per-deep-link-action confirm-modal toggles.  When a field is `true`
/// (the default for all of them) the dispatcher renders an "Are you sure?"
/// modal explaining the action before doing anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmConfig {
    #[serde(default = "default_true")]
    pub repo_open:       bool,
    #[serde(default = "default_true")]
    pub commit_jump:     bool,
    #[serde(default = "default_true")]
    pub branch_checkout: bool,
    #[serde(default = "default_true")]
    pub branch_worktree: bool,
    #[serde(default = "default_true")]
    pub mr_open:         bool,
    #[serde(default = "default_true")]
    pub pipeline_open:   bool,
}

fn default_true() -> bool { true }

impl Default for ConfirmConfig {
    fn default() -> Self {
        Self {
            repo_open:       true,
            commit_jump:     true,
            branch_checkout: true,
            branch_worktree: true,
            mr_open:         true,
            pipeline_open:   true,
        }
    }
}

impl Default for DeepLinkConfig {
    fn default() -> Self {
        Self {
            enabled:              false,
            enable:               EnableConfig::default(),
            cross_workspace_strategy: CrossWorkspaceStrategy::default(),
            confirm:              ConfirmConfig::default(),
            checkout_as_worktree: false,
            worker_base_url:      default_worker_base_url(),
        }
    }
}

// ---------------------------------------------------------------------------
// Cold-start URL buffer
// ---------------------------------------------------------------------------

/// Holds deep-link URLs received between app start and the moment the
/// frontend declares itself ready to handle them.  Without this, a
/// `arbor://…` clicked while Arbor is closed would race the webview's
/// `listen('arbor://deep-link', …)` registration and silently disappear.
///
/// Once `mark_ready_and_flush` is called from the frontend, every subsequent
/// URL is emitted directly without buffering.
#[derive(Default)]
pub struct DeepLinkBuffer {
    ready:   AtomicBool,
    pending: Mutex<Vec<String>>,
}

impl DeepLinkBuffer {
    /// Either emit the URL immediately (frontend ready) or stash it for
    /// later flushing.
    pub fn push_or_emit(&self, app: &AppHandle, url: String) {
        if self.ready.load(Ordering::Relaxed) {
            let _ = app.emit("arbor://deep-link", url);
            return;
        }
        if let Ok(mut p) = self.pending.lock() {
            p.push(url);
        }
    }

    /// Flag the frontend as ready and drain whatever was buffered.  Called
    /// exactly once from `AppShell.onMount` via the `deep_link_ready` IPC.
    pub fn mark_ready_and_flush(&self, app: &AppHandle) {
        self.ready.store(true, Ordering::Relaxed);
        if let Ok(mut p) = self.pending.lock() {
            for url in p.drain(..) {
                let _ = app.emit("arbor://deep-link", url);
            }
        }
    }
}
