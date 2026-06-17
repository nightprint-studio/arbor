//! `deep_link` domain — leaf subset of the `arbor://…` deep-link router,
//! routed through the platform backend.
//!
//! Only the host-agnostic handlers live here:
//!
//!   * **Lookup** — `find_repo_by_remote_url` matches an incoming URL against
//!     the registry using a fuzzy canonical key (host/owner/repo), then
//!     reports which workspaces own that repo. Pure registry/workspace reads.
//!
//!   * **Configuration** — get/set the `[deep_link]` section of `config.toml`.
//!
//! The `AppHandle`-coupled handlers (`deep_link_ready`, `dispatch_deep_link`)
//! stay inline in the command module: they flush the cold-start buffer, manage
//! windows, and emit `arbor://…` events, so they keep the Tauri shell.

use serde::Serialize;

use crate::config::app_config::{self, AppConfig};
use crate::deep_link::DeepLinkConfig;
use crate::error::AppError;
use crate::git::url::{canonical_key, probe_origin_url};
use crate::ipc::platform;
use crate::workspace::registry as registry_io;
use crate::AppState;

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

/// Outcome of matching a deep-link URL against local state.
#[derive(Debug, Serialize)]
pub struct DeepLinkLookup {
    /// Registry id of the matching repo, if any.
    pub repo_id: Option<String>,
    /// Local filesystem path of that repo (when matched). May or may not
    /// exist on disk — the frontend re-validates before opening.
    pub repo_path: Option<String>,
    /// Display name from the registry (when matched).
    pub display_name: Option<String>,
    /// Workspace ids that own the matched repo, in user-defined order.
    /// Empty when the repo is registered but not in any workspace, or when
    /// no match was found.
    pub workspace_ids: Vec<String>,
    /// True when the active workspace is among `workspace_ids`.
    pub in_active_workspace: bool,
    /// The active workspace id at the time of the lookup (echoed back so
    /// the frontend doesn't have to do a separate IPC).
    pub active_workspace_id: Option<String>,
}

#[platform::handler(program = "platform")]
fn find_repo_by_remote_url(state: &AppState, url: String) -> Result<DeepLinkLookup, AppError> {
    let key = canonical_key(&url);
    let active_workspace_id = state.lock_workspaces()?.active_workspace_id.clone();

    // Scan the registry for an entry whose stored remote_url canonicalises
    // to the same key as the incoming URL.  Entries with `remote_url = None`
    // (legacy "Open folder…" registrations made before the registration code
    // started auto-probing) get a one-time backfill: probe `origin` from
    // disk, persist the result, then re-check the match.  Without this,
    // existing user registries silently miss every deep-link.
    let matched: Option<(String, String, String)> = (|| {
        let key = key.as_ref()?;
        let mut reg = state.lock_repo_registry().ok()?;
        let mut dirty = false;
        let mut hit: Option<(String, String, String)> = None;
        let entries = reg.list();
        for entry in entries {
            // Resolve the URL to compare: stored value, or a fresh probe if missing.
            let url_to_check: Option<String> = match entry.remote_url.as_deref() {
                Some(u) => Some(u.to_string()),
                None => {
                    let probed = probe_origin_url(std::path::Path::new(&entry.path));
                    if let Some(ref u) = probed {
                        // Backfill so subsequent lookups skip the CLI hop.
                        if reg.set_remote_url(&entry.id, Some(u.clone())).is_ok() {
                            dirty = true;
                        }
                    }
                    probed
                }
            };
            if let Some(rurl) = url_to_check {
                if canonical_key(&rurl).as_deref() == Some(key.as_str()) {
                    hit = Some((entry.id, entry.path, entry.display_name));
                    break;
                }
            }
        }
        if dirty {
            let _ = registry_io::save(&reg);
        }
        hit
    })();

    let Some((repo_id, repo_path, display_name)) = matched else {
        return Ok(DeepLinkLookup {
            repo_id: None,
            repo_path: None,
            display_name: None,
            workspace_ids: Vec::new(),
            in_active_workspace: false,
            active_workspace_id,
        });
    };

    // Find every workspace that lists this repo, preserving the user's order
    // (Scratch always last via WorkspaceStore::ordered).
    let store = state.lock_workspaces()?;
    let workspace_ids: Vec<String> = store
        .ordered()
        .into_iter()
        .filter(|w| w.repo_ids.iter().any(|id| id == &repo_id))
        .map(|w| w.id)
        .collect();

    let in_active_workspace = active_workspace_id
        .as_ref()
        .map(|aw| workspace_ids.iter().any(|w| w == aw))
        .unwrap_or(false);

    Ok(DeepLinkLookup {
        repo_id: Some(repo_id),
        repo_path: Some(repo_path),
        display_name: Some(display_name),
        workspace_ids,
        in_active_workspace,
        active_workspace_id,
    })
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn get_deep_link_config(state: &AppState) -> Result<DeepLinkConfig, AppError> {
    Ok(state.lock_config()?.deep_link.clone())
}

#[platform::handler(program = "platform")]
fn set_deep_link_config(state: &AppState, config: DeepLinkConfig) -> Result<(), AppError> {
    let snapshot: AppConfig = {
        let mut c = state.lock_config()?;
        c.deep_link = config;
        c.clone()
    };
    app_config::save(&snapshot)?;
    Ok(())
}
