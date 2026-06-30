//! User-added marketplace source pointers.
//!
//! The file is a flat list of pointers — *no* resolved metadata. The
//! actual plugin name / version / permissions are resolved over the
//! network by [`crate::custom::resolve_custom_source`] and cached
//! separately in `marketplace_custom.json` (see [`crate::cache`]) so an
//! offline boot still has something to show.

use serde::{Deserialize, Serialize};

use crate::paths;

// ---------------------------------------------------------------------------
// File schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRegistry {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// One entry per user-added source. A source can resolve to one (root /
    /// subpath modes) or many (index.json mode) plugins — the resolver
    /// decides.
    #[serde(default, rename = "source")]
    pub sources: Vec<UserSource>,
}

impl Default for UserRegistry {
    fn default() -> Self {
        Self { schema_version: default_schema_version(), sources: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSource {
    /// GitHub repo URL — `https://github.com/{owner}/{repo}` (with or
    /// without `.git` / trailing slash). The resolver validates the shape.
    pub repo: String,
    /// Git ref (branch / tag / SHA). `None` → resolver picks `main`.
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    /// When set, forces *subpath mode*: the resolver fetches
    /// `{subpath}/plugin.toml` directly and skips root / index detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    /// Optional commit SHA pin — defends against tag-hijack on custom
    /// sources. Installer compares the resolved ref against this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_sha: Option<String>,
    /// Manual override shown until the first resolve succeeds. Useful for
    /// cold offline boot of a freshly-added source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_schema_version() -> u32 { 1 }

// ---------------------------------------------------------------------------
// Read / write
// ---------------------------------------------------------------------------

pub fn load() -> UserRegistry {
    let p = paths::user_registry_file();
    if !p.exists() { return UserRegistry::default(); }
    match std::fs::read_to_string(&p) {
        Ok(s) => match toml::from_str(&s) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("user_registry.toml parse failed: {e} — using empty");
                UserRegistry::default()
            }
        },
        Err(e) => {
            tracing::warn!("user_registry.toml read failed: {e} — using empty");
            UserRegistry::default()
        }
    }
}

pub fn save(reg: &UserRegistry) {
    let p = paths::user_registry_file();
    if let Some(parent) = p.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("user_registry create_dir_all failed: {e}");
            return;
        }
    }
    match toml::to_string_pretty(reg) {
        Ok(s) => {
            if let Err(e) = std::fs::write(&p, s) {
                tracing::warn!("user_registry write {p:?} failed: {e}");
            }
        }
        Err(e) => tracing::warn!("user_registry serialise failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Mutations — composite key is (repo, subpath) so the same repo can host
// multiple distinct entries pointing at different subpaths.
// ---------------------------------------------------------------------------

pub fn add(source: UserSource) {
    let mut reg = load();
    if reg.sources.iter().any(|s| same_pointer(s, &source)) { return; }
    reg.sources.push(source);
    save(&reg);
}

pub fn remove(repo: &str, subpath: Option<&str>) -> bool {
    let mut reg = load();
    let before = reg.sources.len();
    reg.sources.retain(|s| !(s.repo == repo && s.subpath.as_deref() == subpath));
    let changed = reg.sources.len() != before;
    if changed { save(&reg); }
    changed
}

fn same_pointer(a: &UserSource, b: &UserSource) -> bool {
    a.repo == b.repo && a.subpath == b.subpath
}
