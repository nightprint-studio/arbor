//! Repo registry (`repos.json`) — owned **out-of-process** by corvus-be.
//!
//! Ported from the shell's `crate::workspace::registry` (`AppError` → `String`;
//! `AppError::Other`'s wire shape is `#[error("{0}")]`, so the bare format string
//! the `SplitBroker` re-wraps is byte-identical). The **file is the single source
//! of truth**: every access reloads it (the shell keeps a reload-on-access copy
//! for its own consumers — deep-link router, missing-repo flow, ns_shell — and
//! writes it from the other process), so an in-memory cache would let the two
//! drift. corvus-be can't compute the profile-aware path itself, so the shell
//! pushes it through the `repo_registry_path` config section. Writes go through
//! [`mutate`] (reload → mutate → save, under the lock).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex, MutexGuard};

use corvus_core::prelude::CorvusState;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single repository known to Arbor. The registry is the sole owner of the
/// physical path — every workspace references entries by their UUID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRegistryEntry {
    pub id:           String,
    pub path:         String,
    #[serde(default)]
    pub remote_url:   Option<String>,
    pub display_name: String,
}

/// On-disk shape of `repos.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    entries: Vec<RepoRegistryEntry>,
}

/// In-memory registry. Kept as a HashMap for O(1) lookup by id; iteration order
/// is not preserved (callers that need ordering go through workspaces).
#[derive(Debug, Default, Clone)]
pub struct RepoRegistry {
    entries: HashMap<String, RepoRegistryEntry>,
}

/// Normalise a path for *equality comparison* only — separator + Windows
/// case-insensitivity — so `C:\foo\bar` and `C:/foo/bar/` are the same entry.
/// The stored path keeps the first registration's spelling.
fn normalize_path_for_compare(p: &str) -> String {
    let s: String = p.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

impl RepoRegistry {
    pub fn new() -> Self { Self::default() }

    /// Upsert by path (normalised comparison). The stored `path` keeps the
    /// spelling of the *first* registration so external references stay valid.
    pub fn upsert_by_path(
        &mut self,
        path: &str,
        remote_url: Option<String>,
        fallback_name: &str,
    ) -> String {
        let target = normalize_path_for_compare(path);
        if let Some(existing) = self.entries.values()
            .find(|e| normalize_path_for_compare(&e.path) == target)
        {
            let id = existing.id.clone();
            if existing.remote_url.is_none() && remote_url.is_some() {
                if let Some(e) = self.entries.get_mut(&id) {
                    e.remote_url = remote_url;
                }
            }
            return id;
        }
        let id = Uuid::new_v4().to_string();
        self.entries.insert(id.clone(), RepoRegistryEntry {
            id:           id.clone(),
            path:         path.to_string(),
            remote_url,
            display_name: fallback_name.to_string(),
        });
        id
    }

    /// Register a "pending" repo — declared via name + optional remote URL but
    /// not yet on disk. Stored with an empty `path`; never dedupes.
    pub fn insert_pending(&mut self, remote_url: Option<String>, name: &str) -> String {
        let id = Uuid::new_v4().to_string();
        self.entries.insert(id.clone(), RepoRegistryEntry {
            id:           id.clone(),
            path:         String::new(),
            remote_url,
            display_name: name.to_string(),
        });
        id
    }

    pub fn get(&self, id: &str) -> Option<&RepoRegistryEntry> { self.entries.get(id) }

    pub fn find_by_path(&self, path: &str) -> Option<&RepoRegistryEntry> {
        let target = normalize_path_for_compare(path);
        self.entries.values()
            .find(|e| normalize_path_for_compare(&e.path) == target)
    }

    pub fn find_by_remote_url(&self, url: &str) -> Option<&RepoRegistryEntry> {
        self.entries.values().find(|e| e.remote_url.as_deref() == Some(url))
    }

    pub fn list(&self) -> Vec<RepoRegistryEntry> {
        let mut v: Vec<_> = self.entries.values().cloned().collect();
        v.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
        v
    }

    pub fn remove(&mut self, id: &str) -> Option<RepoRegistryEntry> { self.entries.remove(id) }

    pub fn set_path(&mut self, id: &str, path: String) -> Result<(), String> {
        let entry = self.entries.get_mut(id).ok_or_else(|| format!("repo not found: {id}"))?;
        entry.path = path;
        Ok(())
    }

    pub fn set_display_name(&mut self, id: &str, name: String) -> Result<(), String> {
        let entry = self.entries.get_mut(id).ok_or_else(|| format!("repo not found: {id}"))?;
        entry.display_name = name;
        Ok(())
    }

    pub fn set_remote_url(&mut self, id: &str, url: Option<String>) -> Result<(), String> {
        let entry = self.entries.get_mut(id).ok_or_else(|| format!("repo not found: {id}"))?;
        entry.remote_url = url;
        Ok(())
    }

    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Replace contents (used on reload).
    fn replace_all(&mut self, list: Vec<RepoRegistryEntry>) {
        self.entries.clear();
        for e in list {
            self.entries.insert(e.id.clone(), e);
        }
    }
}

// ── Persistence — the file is the single source of truth ──────────────────────

static REGISTRY: LazyLock<Mutex<RepoRegistry>> = LazyLock::new(Default::default);

fn registry_path(state: &CorvusState) -> Option<String> {
    state
        .config("repo_registry_path")
        .and_then(|v| v.as_str().map(String::from))
}

fn load_from(path: &Path) -> RepoRegistry {
    let mut reg = RepoRegistry::new();
    if let Ok(content) = std::fs::read_to_string(path) {
        let file: RegistryFile = serde_json::from_str(&content).unwrap_or_default();
        reg.replace_all(file.entries);
    }
    reg
}

fn load_path(path: &Option<String>) -> RepoRegistry {
    match path.as_deref() {
        Some(p) => load_from(Path::new(p)),
        None => RepoRegistry::new(),
    }
}

/// Read-access — the guard holds a snapshot freshly read from the file. Slow
/// callers (per-repo git probes) should clone the entries and drop the guard
/// before probing, exactly as the shell did under its lock.
pub fn registry(state: &CorvusState) -> MutexGuard<'static, RepoRegistry> {
    let path = registry_path(state);
    let mut reg = REGISTRY.lock().unwrap_or_else(|p| p.into_inner());
    *reg = load_path(&path);
    reg
}

/// Reload-fresh → mutate → persist, all under the lock (so corvus-be's own
/// mutations serialize and each sees the latest file, incl. shell writes).
pub fn mutate<T>(
    state: &CorvusState,
    f: impl FnOnce(&mut RepoRegistry) -> Result<T, String>,
) -> Result<T, String> {
    let path = registry_path(state);
    let mut reg = REGISTRY.lock().unwrap_or_else(|p| p.into_inner());
    *reg = load_path(&path);
    let result = f(&mut reg)?;
    save_to(&reg, &path)?;
    Ok(result)
}

fn save_to(reg: &RepoRegistry, path: &Option<String>) -> Result<(), String> {
    let Some(path) = path else { return Ok(()); };
    let mut entries: Vec<_> = reg.entries.values().cloned().collect();
    entries.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    let content = serde_json::to_string_pretty(&RegistryFile { entries })
        .map_err(|e| format!("repo registry: serialize failed: {e}"))?;
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}
