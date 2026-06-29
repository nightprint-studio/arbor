//! `libraries` domain — merula **external libraries**: GitHub-hosted `.merula`
//! modules a project depends on, declared in `merula.toml`'s `[libraries]` table,
//! fetched (pinned to a commit SHA) into a shared content-addressed cache, and
//! imported via `import { … } from "$lib/<name>/<file>"`.
//!
//! ```toml
//! # merula.toml
//! [libraries]
//! drums = "github:octocat/merula-drums@v1.2"   # owner/repo[@ref]; ref optional
//! ```
//!
//! This module owns the **read** surface: parsing source specs, reading the
//! manifest `[libraries]` table + the `merula.lock`, the content-addressed cache
//! layout, the `merula_libraries` listing handler, and [`resolve_dirs`] (which the
//! eval domain resolves `$lib/<name>/…` imports against). The **sync** (resolve
//! ref → SHA, download the zipball, rewrite the lock) is job-tracked and lives in
//! [`crate::libraries_sync`] — it reuses the spec / lock / cache helpers kept
//! `pub` here. Ported from the shell's `src-tauri/src/merula/libraries.rs`, split
//! along the read/job seam, with the `AppError` mapped to the wire `String`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use arbor_core::prelude::merula_data_dir;

use crate::state::MerulaState;

// ── Source spec ──────────────────────────────────────────────────────────────

/// A parsed `"github:owner/repo@ref"` (the `github:` prefix and `@ref` are
/// optional; a missing ref means the repo's default branch head).
pub struct GithubSource {
    pub owner: String,
    pub repo: String,
    pub git_ref: String,
}

/// Parse a library source spec. Accepts `github:owner/repo@ref`, `owner/repo@ref`,
/// or `owner/repo` (ref defaults to `HEAD`).
pub fn parse_source(spec: &str) -> Result<GithubSource, String> {
    let s = spec.trim();
    let s = s.strip_prefix("github:").unwrap_or(s).trim();
    let (path, git_ref) = match s.split_once('@') {
        Some((p, r)) => (p.trim(), r.trim().to_string()),
        None => (s, "HEAD".to_string()),
    };
    let (owner, repo) = path
        .split_once('/')
        .ok_or_else(|| format!("invalid library source `{spec}` (expected owner/repo)"))?;
    if owner.is_empty() || repo.is_empty() || git_ref.is_empty() {
        return Err(format!("invalid library source `{spec}`"));
    }
    Ok(GithubSource { owner: owner.to_string(), repo: repo.to_string(), git_ref })
}

// ── Manifest `[libraries]` ────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct LibManifest {
    #[serde(default)]
    libraries: BTreeMap<String, String>,
}

/// The libraries declared in `<dir>/merula.toml` (name → source spec), or empty
/// when there's no manifest / no `[libraries]` table.
pub fn declared(dir: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(dir.join("merula.toml")) else {
        return BTreeMap::new();
    };
    toml::from_str::<LibManifest>(&text).map(|m| m.libraries).unwrap_or_default()
}

// ── Lock file (`merula.lock`) ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
pub struct LockFile {
    #[serde(default)]
    pub libraries: BTreeMap<String, LockEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LockEntry {
    /// The source spec as declared (so a changed spec re-resolves).
    pub source: String,
    /// The resolved commit SHA the cache dir is keyed by.
    pub sha: String,
}

fn lock_path(dir: &Path) -> PathBuf {
    dir.join("merula.lock")
}

pub fn read_lock(dir: &Path) -> LockFile {
    std::fs::read_to_string(lock_path(dir))
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn write_lock(dir: &Path, lock: &LockFile) -> Result<(), String> {
    let header = "# merula.lock — auto-generated. Pins each [libraries] entry to a commit SHA.\n\n";
    let body = toml::to_string_pretty(lock).map_err(|e| format!("serialize lock: {e}"))?;
    std::fs::write(lock_path(dir), format!("{header}{body}"))
        .map_err(|e| format!("write merula.lock: {e}"))
}

// ── Cache ─────────────────────────────────────────────────────────────────────

/// Root of the shared library cache (`…/merula/libraries`).
pub fn cache_root() -> PathBuf {
    merula_data_dir().join("libraries")
}

/// The (content-addressed) cache dir for a commit `sha`.
pub fn cache_dir(sha: &str) -> PathBuf {
    cache_root().join(sha)
}

/// Map every **locked + present** library to its cache dir (name → dir) — what the
/// import loader resolves `$lib/<name>/…` against. Skips entries whose cache is
/// missing (not yet synced) so the import surfaces a clear "not synced" error.
pub fn resolve_dirs(project_dir: &Path) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    for (name, entry) in read_lock(project_dir).libraries {
        let dir = cache_dir(&entry.sha);
        if dir.is_dir() {
            out.insert(name, dir);
        }
    }
    out
}

// ── Status (FE) ───────────────────────────────────────────────────────────────

/// One library's state for the FE: declared source + (if locked) the pinned SHA
/// and whether its cache is present.
#[derive(Serialize)]
pub struct LibraryStatus {
    pub name: String,
    pub source: String,
    pub sha: Option<String>,
    pub synced: bool,
}

/// The declared libraries with their lock/sync state.
#[arbor_rpc::handler]
fn merula_libraries(_ctx: &MerulaState, project_dir: String) -> Result<Vec<LibraryStatus>, String> {
    let dir = PathBuf::from(project_dir);
    let lock = read_lock(&dir);
    Ok(declared(&dir)
        .into_iter()
        .map(|(name, source)| {
            let locked = lock.libraries.get(&name);
            // Synced when the lock matches the declared source AND the cache exists.
            let sha = locked.filter(|l| l.source == source).map(|l| l.sha.clone());
            let synced = sha.as_deref().map(|s| cache_dir(s).is_dir()).unwrap_or(false);
            LibraryStatus { name, source, sha, synced }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_source_accepts_prefixed_and_ref() {
        let s = parse_source("github:octocat/merula-drums@v1.2").unwrap();
        assert_eq!(s.owner, "octocat");
        assert_eq!(s.repo, "merula-drums");
        assert_eq!(s.git_ref, "v1.2");
    }

    #[test]
    fn parse_source_defaults_ref_to_head() {
        let s = parse_source("octocat/merula-drums").unwrap();
        assert_eq!(s.owner, "octocat");
        assert_eq!(s.repo, "merula-drums");
        assert_eq!(s.git_ref, "HEAD");
    }

    #[test]
    fn parse_source_rejects_missing_repo() {
        assert!(parse_source("octocat").is_err());
        assert!(parse_source("github:").is_err());
    }
}
