use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use git2::{Oid, Repository};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::error::{AppError, Result};

pub const NOTES_REF: &str = "refs/notes/arbor/tickets";
const LINKS_TOML_FILE: &str = ".arbor/links.toml";

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    #[default]
    GitNotes,
    LinksToml,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkSource {
    AutoMessage,
    AutoBranch,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketLink {
    pub ticket_id: String,
    pub tracker:   String,
    pub source:    LinkSource,
}

/// Effective configuration resolved from global + per-repo overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketLinkConfig {
    #[serde(default)]
    pub storage:        StorageBackend,
    pub tracker:        Option<String>,
    #[serde(default = "default_true")]
    pub auto_parse:     bool,
    #[serde(default = "default_true")]
    pub warn_push:      bool,
    /// Optional custom regex (one capture group) that overrides the tracker default.
    pub custom_pattern: Option<String>,
}

pub fn default_true() -> bool { true }

impl Default for TicketLinkConfig {
    fn default() -> Self {
        Self {
            storage:        StorageBackend::default(),
            tracker:        None,
            auto_parse:     true,
            warn_push:      true,
            custom_pattern: None,
        }
    }
}

// ── Per-tab in-memory cache ───────────────────────────────────────────────────

/// Caches ticket link lookups to avoid redundant git-notes reads or TOML
/// parses across multiple `get_commit_ticket_links` calls (e.g. on scroll).
#[derive(Debug, Default)]
pub struct TicketLinkCache {
    /// Immutable: commit messages never change → valid forever once parsed.
    pub auto_parsed: HashMap<String, Vec<TicketLink>>,
    /// Manual links already loaded from storage (git notes / links.toml).
    pub manual: HashMap<String, Vec<TicketLink>>,
    /// SHAs that have already been queried from the backing store.
    /// Avoids repeat I/O even when the result was empty.
    pub manual_checked: HashSet<String>,
    /// For links.toml: the full parsed file kept in RAM.
    /// `None` = not yet loaded; `Some(_)` = loaded (may be empty map).
    pub toml_all: Option<HashMap<String, Vec<TicketLink>>>,
}

impl TicketLinkCache {
    /// Invalidate cached manual links for a single SHA (after add/remove).
    pub fn invalidate_manual(&mut self, sha: &str) {
        self.manual.remove(sha);
        self.manual_checked.remove(sha);
        // Force a re-read of the TOML file on the next query.
        self.toml_all = None;
    }

    /// Invalidate ALL manual link caches (e.g. after the storage backend changes).
    pub fn invalidate_all_manual(&mut self) {
        self.manual.clear();
        self.manual_checked.clear();
        self.toml_all = None;
    }
}

// ── Regex helpers (compiled once) ─────────────────────────────────────────────

fn linear_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Matches Linear-style identifiers: PROJ-123, ENG-456, …
    RE.get_or_init(|| Regex::new(r"\b([A-Z][A-Z0-9]*-\d+)\b").unwrap())
}

fn github_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Matches GitHub / GitLab issue references: #123
    RE.get_or_init(|| Regex::new(r"#(\d+)\b").unwrap())
}

// ── Auto-parsing ──────────────────────────────────────────────────────────────

/// Extract ticket references from a text string.
///
/// `custom_re` — when `Some`, takes full precedence over the tracker default.
/// It must contain exactly one capture group (the ticket ID).
pub fn parse_text(text: &str, tracker: &str, source: LinkSource, custom_re: Option<&Regex>) -> Vec<TicketLink> {
    let mut seen: HashSet<String> = HashSet::new();

    let re: &Regex = if let Some(r) = custom_re {
        r
    } else {
        match tracker {
            "linear" | "jira"      => linear_re(),
            "github" | "gitlab"    => github_re(),
            _                      => return vec![],
        }
    };

    // For github/gitlab the ID is prefixed with "#"; for everything else use raw capture.
    let needs_hash = custom_re.is_none() && matches!(tracker, "github" | "gitlab");

    re.captures_iter(text)
        .filter_map(|cap| {
            let raw = cap.get(1)?.as_str().to_string();
            let id  = if needs_hash { format!("#{raw}") } else { raw };
            if seen.insert(id.clone()) {
                Some(TicketLink { ticket_id: id, tracker: tracker.to_string(), source: source.clone() })
            } else {
                None
            }
        })
        .collect()
}

// ── Git notes ─────────────────────────────────────────────────────────────────

/// Read ticket links stored as a JSON array in a git note for `sha`.
pub fn read_git_notes(repo: &Repository, sha: &str) -> Result<Vec<TicketLink>> {
    let oid = Oid::from_str(sha).map_err(AppError::Git)?;
    match repo.find_note(Some(NOTES_REF), oid) {
        Ok(note) => {
            let msg = note.message().unwrap_or("").trim();
            if msg.is_empty() { return Ok(vec![]); }
            Ok(serde_json::from_str(msg).unwrap_or_default())
        }
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(vec![]),
        Err(e) => Err(AppError::Git(e)),
    }
}

/// Write (or delete) the ticket-link note for `sha`.
pub fn write_git_notes(repo: &Repository, sha: &str, links: &[TicketLink]) -> Result<()> {
    let oid = Oid::from_str(sha).map_err(AppError::Git)?;
    let sig = repo.signature().map_err(AppError::Git)?;
    if links.is_empty() {
        // Remove the note entirely when the link list is empty.
        let _ = repo.note_delete(oid, Some(NOTES_REF), &sig, &sig);
        return Ok(());
    }
    let json = serde_json::to_string(links)?;
    repo.note(&sig, &sig, Some(NOTES_REF), oid, &json, true)
        .map_err(AppError::Git)?;
    Ok(())
}

/// Returns `true` if the repo's git config already has a fetch/push refspec
/// that includes `refs/notes/arbor/tickets` (so notes survive a push/fetch).
pub fn check_notes_push_refspec(repo: &Repository) -> bool {
    let Ok(config) = repo.config() else { return false };
    for key in &["remote.origin.push", "remote.origin.fetch"] {
        if let Ok(val) = config.get_string(key) {
            if val.contains("notes") { return true; }
        }
    }
    false
}

// ── links.toml ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct LinksToml {
    #[serde(default)]
    links: Vec<TomlLinkEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
struct TomlLinkEntry {
    sha:       String,
    ticket_id: String,
    tracker:   String,
}

pub fn links_toml_path(workdir: &Path) -> PathBuf {
    workdir.join(LINKS_TOML_FILE)
}

/// Load the entire `.arbor/links.toml` into a SHA-keyed map.
pub fn read_all_toml_links(workdir: &Path) -> Result<HashMap<String, Vec<TicketLink>>> {
    let path = links_toml_path(workdir);
    if !path.exists() { return Ok(HashMap::new()); }
    let content = std::fs::read_to_string(&path)?;
    let parsed: LinksToml = toml::from_str(&content).unwrap_or_default();
    let mut map: HashMap<String, Vec<TicketLink>> = HashMap::new();
    for entry in parsed.links {
        map.entry(entry.sha).or_default().push(TicketLink {
            ticket_id: entry.ticket_id,
            tracker:   entry.tracker,
            source:    LinkSource::Manual,
        });
    }
    Ok(map)
}

/// Append a manual link to `.arbor/links.toml` (idempotent).
pub fn add_toml_link(workdir: &Path, sha: &str, ticket_id: &str, tracker: &str) -> Result<()> {
    let path = links_toml_path(workdir);
    let mut parsed: LinksToml = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        LinksToml::default()
    };
    if !parsed.links.iter().any(|e| e.sha == sha && e.ticket_id == ticket_id) {
        parsed.links.push(TomlLinkEntry {
            sha:       sha.to_string(),
            ticket_id: ticket_id.to_string(),
            tracker:   tracker.to_string(),
        });
        let content = toml::to_string_pretty(&parsed)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(())
}

/// Remove a specific link entry from `.arbor/links.toml`.
pub fn remove_toml_link(workdir: &Path, sha: &str, ticket_id: &str) -> Result<()> {
    let path = links_toml_path(workdir);
    if !path.exists() { return Ok(()); }
    let content = std::fs::read_to_string(&path)?;
    let mut parsed: LinksToml = toml::from_str(&content).unwrap_or_default();
    parsed.links.retain(|e| !(e.sha == sha && e.ticket_id == ticket_id));
    let content = toml::to_string_pretty(&parsed)?;
    std::fs::write(&path, content)?;
    Ok(())
}
