use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use crate::git::url::probe_origin_url;
use super::{registry, store, RepoRegistry, WorkspaceStore, SCRATCH_ID};

// ---------------------------------------------------------------------------
// Legacy session.json — migrated to workspace snapshots on first run.
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, Serialize)]
struct LegacySessionTab {
    path: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct LegacySession {
    #[serde(default)]
    tabs: Vec<LegacySessionTab>,
    #[serde(default)]
    active_path: Option<String>,
}

fn legacy_session_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("session.json")
}

fn read_legacy_session() -> Option<LegacySession> {
    let path = legacy_session_path();
    if !path.exists() { return None; }
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn delete_legacy_session() {
    let path = legacy_session_path();
    if path.exists() { let _ = std::fs::remove_file(path); }
}

fn display_name_for(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repository".to_string())
}

// ---------------------------------------------------------------------------
// Migration result — reported back to the frontend so the welcome screen
// can list the repos that were ingested.
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
pub struct MigrationReport {
    /// True when at least one of the state files already exists — i.e. a
    /// previous install has already been migrated.  The caller uses this as
    /// a "skip the welcome wizard" signal.
    pub already_migrated: bool,
    /// IDs of repos that were newly registered during this migration.
    pub added_repo_ids:   Vec<String>,
    /// IDs of repos that were already registered (deduped by path).
    pub existing_repo_ids: Vec<String>,
    /// The id that was active in the legacy session, if any.
    pub active_repo_id:   Option<String>,
}

/// Run the one-shot migration.  Safe to call multiple times: the presence
/// of `repos.json` and `workspaces.json` is used as the "already migrated"
/// marker, in which case this is a no-op.
///
/// Returns a report the UI can show on the first post-upgrade launch.
pub fn run_if_needed() -> MigrationReport {
    let registry_exists  = registry::registry_path().exists();
    let workspaces_exist = store::store_path().exists();

    // Already migrated — nothing to do.
    if registry_exists && workspaces_exist {
        return MigrationReport { already_migrated: true, ..Default::default() };
    }

    let legacy = match read_legacy_session() {
        Some(s) => s,
        None => {
            // Seed empty files so we never run again.
            let reg = RepoRegistry::new();
            let _ = registry::save(&reg);
            let store = WorkspaceStore::default();
            let _ = store::save(&store);
            return MigrationReport::default();
        }
    };

    let mut registry = if registry_exists { registry::load() } else { RepoRegistry::new() };
    let mut store    = if workspaces_exist { store::load() }    else { WorkspaceStore::default() };

    let mut added:    Vec<String> = Vec::new();
    let mut existing: Vec<String> = Vec::new();
    let mut active_repo_id: Option<String> = None;

    // Dedup by path — legacy session files occasionally duplicate.
    let mut seen_paths = std::collections::HashSet::<String>::new();

    for tab in &legacy.tabs {
        if !seen_paths.insert(tab.path.clone()) { continue; }
        if !Path::new(&tab.path).exists() { continue; }
        let was_registered = registry.find_by_path(&tab.path).is_some();
        let name = if tab.name.is_empty() { display_name_for(&tab.path) } else { tab.name.clone() };
        let url  = probe_origin_url(Path::new(&tab.path));
        let id = registry.upsert_by_path(&tab.path, url, &name);
        if was_registered { existing.push(id.clone()); } else { added.push(id.clone()); }
        if let Some(active_path) = &legacy.active_path {
            if active_path == &tab.path { active_repo_id = Some(id.clone()); }
        }
        // Populate Scratch with every migrated repo; the user will move
        // them into named workspaces from the welcome screen / modal.
        let _ = store.add_repo(SCRATCH_ID, &id);
    }

    let _ = registry::save(&registry);
    let _ = store::save(&store);

    // Seed a Scratch snapshot so tab restoration on first launch matches
    // the user's previous session.
    {
        use super::snapshot::{TabSnapshot, CrossWsTabRef};
        let open_tab_ids: Vec<String> = added.iter().chain(existing.iter()).cloned().collect();
        let active_tab_id = active_repo_id.clone().or_else(|| open_tab_ids.first().cloned());
        let _ = super::snapshot::save(SCRATCH_ID, &TabSnapshot {
            open_tab_ids,
            active_tab_id,
            cross_ws_tabs: Vec::<CrossWsTabRef>::new(),
            tab_meta:      Vec::new(),
        });
    }

    delete_legacy_session();

    MigrationReport {
        already_migrated: false,
        added_repo_ids:   added,
        existing_repo_ids: existing,
        active_repo_id,
    }
}
