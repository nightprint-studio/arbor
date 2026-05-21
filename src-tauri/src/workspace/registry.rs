use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};

/// A single repository known to Arbor.  The registry is the sole owner of
/// the physical path — every workspace references entries by their UUID, so
/// renaming the display name or re-locating the repo on disk is a one-place
/// edit.
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

/// In-memory registry.  Kept as a HashMap for O(1) lookup by id; iteration
/// order is not preserved (callers that need ordering go through workspaces).
#[derive(Debug, Default, Clone)]
pub struct RepoRegistry {
    entries: HashMap<String, RepoRegistryEntry>,
}

/// Normalise a path for *equality comparison* only.  We keep the original
/// path in storage (so existing snapshots referring to it by exact spelling
/// keep working) but we want `C:\foo\bar` and `C:/foo/bar/` to be the same
/// entry — and on Windows comparison should be case-insensitive too.
fn normalize_path_for_compare(p: &str) -> String {
    let s: String = p.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

impl RepoRegistry {
    pub fn new() -> Self { Self::default() }

    /// Upsert by path.  Comparison is normalised (separator + Windows case-
    /// insensitive) so we don't end up with two entries for the same physical
    /// directory just because one used `\` and the other `/`.  The stored
    /// `path` keeps the spelling of the *first* registration so any external
    /// reference (snapshot files, recent-repos list) stays valid.
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
            // Opportunistically fill in a missing remote URL we just learned.
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

    pub fn set_path(&mut self, id: &str, path: String) -> Result<()> {
        let entry = self.entries.get_mut(id).ok_or_else(|| AppError::Other(format!("repo not found: {id}")))?;
        entry.path = path;
        Ok(())
    }

    pub fn set_display_name(&mut self, id: &str, name: String) -> Result<()> {
        let entry = self.entries.get_mut(id).ok_or_else(|| AppError::Other(format!("repo not found: {id}")))?;
        entry.display_name = name;
        Ok(())
    }

    pub fn set_remote_url(&mut self, id: &str, url: Option<String>) -> Result<()> {
        let entry = self.entries.get_mut(id).ok_or_else(|| AppError::Other(format!("repo not found: {id}")))?;
        entry.remote_url = url;
        Ok(())
    }

    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn registry_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("repos.json")
}

pub fn load() -> RepoRegistry {
    let path = registry_path();
    if !path.exists() { return RepoRegistry::new(); }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("repo registry: read failed, starting empty: {e}");
            return RepoRegistry::new();
        }
    };
    let file: RegistryFile = serde_json::from_str(&content).unwrap_or_default();
    let mut reg = RepoRegistry::new();
    for e in file.entries { reg.entries.insert(e.id.clone(), e); }
    reg
}

pub fn save(reg: &RepoRegistry) -> Result<()> {
    let path = registry_path();
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let mut entries: Vec<_> = reg.entries.values().cloned().collect();
    entries.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    let file = RegistryFile { entries };
    let content = serde_json::to_string_pretty(&file)
        .map_err(|e| AppError::Other(format!("repo registry: serialize failed: {e}")))?;
    std::fs::write(&path, content)?;
    Ok(())
}
