//! IPC surface for the `arbor://…` deep-link router.
//!
//! Three concerns:
//!
//!   * **Lookup** — `find_repo_by_remote_url` matches an incoming URL against
//!     the registry using a fuzzy canonical key (host/owner/repo), then
//!     reports which workspaces own that repo.  The frontend dispatcher
//!     uses the result to decide between switch / open-here / clone-prompt.
//!
//!   * **Cold-start delivery** — `deep_link_ready` lets the frontend tell
//!     the backend that its `arbor://deep-link` listener is mounted, after
//!     which the URL buffer is flushed and future links emit immediately.
//!
//!   * **Configuration** — get/set the `[deep_link]` section of `config.toml`.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::error::Result;
use crate::config::app_config::{self, AppConfig};
use crate::deep_link::DeepLinkConfig;
use crate::git::url::{canonical_key, probe_origin_url};
use crate::workspace::registry as registry_io;

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

/// Outcome of matching a deep-link URL against local state.
#[derive(Debug, Serialize)]
pub struct DeepLinkLookup {
    /// Registry id of the matching repo, if any.
    pub repo_id:           Option<String>,
    /// Local filesystem path of that repo (when matched).  May or may not
    /// exist on disk — the frontend re-validates before opening.
    pub repo_path:         Option<String>,
    /// Display name from the registry (when matched).
    pub display_name:      Option<String>,
    /// Workspace ids that own the matched repo, in user-defined order.
    /// Empty when the repo is registered but not in any workspace, or when
    /// no match was found.
    pub workspace_ids:     Vec<String>,
    /// True when the active workspace is among `workspace_ids`.
    pub in_active_workspace: bool,
    /// The active workspace id at the time of the lookup (echoed back so
    /// the frontend doesn't have to do a separate IPC).
    pub active_workspace_id: Option<String>,
}

#[tauri::command]
pub fn find_repo_by_remote_url(
    state: State<'_, AppState>,
    url:   String,
) -> Result<DeepLinkLookup> {
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
        if dirty { let _ = registry_io::save(&reg); }
        hit
    })();

    let Some((repo_id, repo_path, display_name)) = matched else {
        return Ok(DeepLinkLookup {
            repo_id: None, repo_path: None, display_name: None,
            workspace_ids: Vec::new(),
            in_active_workspace: false,
            active_workspace_id,
        });
    };

    // Find every workspace that lists this repo, preserving the user's order
    // (Scratch always last via WorkspaceStore::ordered).
    let store = state.lock_workspaces()?;
    let workspace_ids: Vec<String> = store.ordered().into_iter()
        .filter(|w| w.repo_ids.iter().any(|id| id == &repo_id))
        .map(|w| w.id)
        .collect();

    let in_active_workspace = active_workspace_id.as_ref()
        .map(|aw| workspace_ids.iter().any(|w| w == aw))
        .unwrap_or(false);

    Ok(DeepLinkLookup {
        repo_id:             Some(repo_id),
        repo_path:           Some(repo_path),
        display_name:        Some(display_name),
        workspace_ids,
        in_active_workspace,
        active_workspace_id,
    })
}

// ---------------------------------------------------------------------------
// Cold-start delivery
// ---------------------------------------------------------------------------

/// Called once by the frontend (`AppShell.onMount`) after it has registered
/// its `arbor://deep-link` event listener.  Drains any URLs that arrived
/// during cold-start and switches the buffer to direct-emit mode.
#[tauri::command]
pub fn deep_link_ready(app: AppHandle) -> Result<()> {
    let state = app.state::<AppState>();
    state.deep_link_buffer.mark_ready_and_flush(&app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_deep_link_config(state: State<'_, AppState>) -> Result<DeepLinkConfig> {
    Ok(state.lock_config()?.deep_link.clone())
}

#[tauri::command]
pub fn set_deep_link_config(
    state: State<'_, AppState>,
    config: DeepLinkConfig,
) -> Result<()> {
    let snapshot: AppConfig = {
        let mut c = state.lock_config()?;
        c.deep_link = config;
        c.clone()
    };
    app_config::save(&snapshot)?;
    Ok(())
}
