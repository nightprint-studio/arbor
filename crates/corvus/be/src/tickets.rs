//! `tickets` (ticket-links) domain — commit ↔ ticket-ID associations, served
//! **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::tickets`), ported onto [`CorvusState`] + the shared
//! `corvus-git` `tickets` module (parse / git-notes / links.toml / regex — the
//! same code the shell wrapped). **No hooks fire** in this domain.
//!
//! The per-tab parse/lookup cache (the in-process `AppState::ticket_caches`)
//! lives here as a process-local [`TICKET_CACHES`] map — corvus-be is its sole
//! owner now, so cache invalidation is local (no cross-process sync). The global
//! `ticket_links` config is read from corvus-be's owned global config
//! ([`crate::corvus_config`]); the per-repo override is read straight from
//! `<workdir>/.arbor/config.toml`
//! (the same direct-read precedent as the gitflow / stats domains). The one write
//! handler (`set_ticket_link_repo_config`) merges the `ticket_links` table into
//! that file, preserving every other section.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use corvus_core::prelude::CorvusState;
use corvus_git::tickets::{
    add_toml_link, check_notes_push_refspec, parse_text, read_all_toml_links, read_git_notes,
    remove_toml_link, write_git_notes, LinkSource, StorageBackend, TicketLink, TicketLinkCache,
    TicketLinkConfig, NOTES_REF,
};
use serde::{Deserialize, Serialize};

use crate::repo::{open, repo_path};

/// Per-tab parse/lookup cache (auto-parsed links, checked manual links, the warm
/// links.toml map). Process-local: corvus-be owns it end-to-end, so every
/// mutation invalidates here with no shell round-trip.
static TICKET_CACHES: LazyLock<Mutex<HashMap<String, TicketLinkCache>>> =
    LazyLock::new(Default::default);

fn caches() -> std::sync::MutexGuard<'static, HashMap<String, TicketLinkCache>> {
    TICKET_CACHES.lock().unwrap_or_else(|p| p.into_inner())
}

// ── Config shapes (wire twins of the shell's app/repo config slices) ──────────

/// The per-repo `[ticket_links]` override (wire twin of the shell's
/// `TicketLinksRepoConfig`), serialized straight into `.arbor/config.toml`.
#[derive(Deserialize, Serialize, Default)]
struct TicketLinksRepoConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    storage: Option<StorageBackend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tracker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_parse: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    custom_pattern: Option<String>,
}

/// The slice of `.arbor/config.toml` the ticket config resolution reads.
#[derive(Deserialize, Default)]
struct RepoTicketSlice {
    #[serde(default)]
    ticket_links: Option<TicketLinksRepoConfig>,
    #[serde(default)]
    issue_tracker: Option<String>,
}

/// The global `ticket_links` config, read from corvus-be's owned global config
/// ([`crate::corvus_config`]) rather than the shell-pushed copy.
fn global_config(state: &CorvusState) -> crate::corvus_config::TicketLinksGlobalConfig {
    crate::corvus_config::load(state).ticket_links
}

fn repo_slice(workdir: &str) -> RepoTicketSlice {
    let path = Path::new(workdir).join(".arbor").join("config.toml");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str::<RepoTicketSlice>(&s).ok())
        .unwrap_or_default()
}

/// Resolve the effective ticket-link config for `tab_id`: global defaults ←
/// overridden by the per-repo `.arbor/config.toml`. Byte-identical merge order to
/// the in-process `effective_config`.
fn effective_config(state: &CorvusState, tab_id: &str) -> Result<TicketLinkConfig, String> {
    let global = global_config(state);
    let workdir = repo_path(state, tab_id)?;
    let repo_cfg = repo_slice(&workdir);

    let storage = repo_cfg
        .ticket_links
        .as_ref()
        .and_then(|c| c.storage.clone())
        .unwrap_or(global.storage);

    // tracker: repo.ticket_links.tracker > repo.issue_tracker (legacy) > None
    let tracker = repo_cfg
        .ticket_links
        .as_ref()
        .and_then(|c| c.tracker.clone())
        .or(repo_cfg.issue_tracker);

    let auto_parse = repo_cfg
        .ticket_links
        .as_ref()
        .and_then(|c| c.auto_parse)
        .unwrap_or(global.auto_parse);

    let custom_pattern = repo_cfg.ticket_links.as_ref().and_then(|c| c.custom_pattern.clone());

    Ok(TicketLinkConfig { storage, tracker, auto_parse, warn_push: global.warn_push, custom_pattern })
}

// ── Input / output DTOs ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CommitQueryItem {
    pub sha: String,
    pub message: String,
    /// Branch/tag names that point at (or are ancestors of) this commit.
    pub refs: Vec<String>,
}

/// A commit associated (manually or via auto-parse) with a ticket.
#[derive(Serialize)]
pub struct LinkedCommitRef {
    pub sha: String,
    pub short_oid: String,
    pub summary: String,
    pub author_name: String,
    pub timestamp: i64,
    pub source: LinkSource,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// Batch-fetch ticket links for a list of visible commits (SHA → links). Only
/// SHAs not already cached trigger I/O.
#[arbor_rpc::handler]
fn get_commit_ticket_links(
    state: &CorvusState,
    tab_id: String,
    commits: Vec<CommitQueryItem>,
) -> Result<HashMap<String, Vec<TicketLink>>, String> {
    if !global_config(state).enabled {
        return Ok(HashMap::new());
    }

    let config = effective_config(state, &tab_id)?;

    // Compile the custom regex once for the whole batch. `captures_len()` counts
    // the whole-match slot, so >1 means ≥1 capture group.
    let custom_compiled: Option<regex::Regex> = config
        .custom_pattern
        .as_deref()
        .and_then(|p| regex::Regex::new(p).ok())
        .filter(|re| re.captures_len() > 1);
    let custom_re: Option<&regex::Regex> = custom_compiled.as_ref();

    // ── Stage 1: check cache, build preliminary result, collect uncached SHAs ──
    let (mut result, need_manual_fetch): (HashMap<String, Vec<TicketLink>>, Vec<String>) = {
        let mut c = caches();
        let cache = c.entry(tab_id.clone()).or_default();

        let mut res: HashMap<String, Vec<TicketLink>> = HashMap::new();
        let mut need_fetch: Vec<String> = vec![];

        for item in &commits {
            let mut links: Vec<TicketLink> = vec![];

            if config.auto_parse {
                if let Some(tracker) = &config.tracker {
                    if let Some(auto) = cache.auto_parsed.get(&item.sha) {
                        links.extend_from_slice(auto);
                    } else {
                        let mut auto = parse_text(&item.message, tracker, LinkSource::AutoMessage, custom_re);
                        for ref_name in &item.refs {
                            for bl in parse_text(ref_name, tracker, LinkSource::AutoBranch, custom_re) {
                                if !auto.iter().any(|l| l.ticket_id == bl.ticket_id) {
                                    auto.push(bl);
                                }
                            }
                        }
                        cache.auto_parsed.insert(item.sha.clone(), auto.clone());
                        links.extend(auto);
                    }
                }
            }

            if cache.manual_checked.contains(&item.sha) {
                if let Some(manual) = cache.manual.get(&item.sha) {
                    links.extend_from_slice(manual);
                }
            } else {
                need_fetch.push(item.sha.clone());
            }

            res.insert(item.sha.clone(), links);
        }

        (res, need_fetch)
    };

    // ── Stage 2: fetch uncached manual links from the backing store ────────────
    if !need_manual_fetch.is_empty() {
        let fetched: HashMap<String, Vec<TicketLink>> = match &config.storage {
            StorageBackend::GitNotes => {
                let repo = open(state, &tab_id)?;
                let mut map = HashMap::new();
                for sha in &need_manual_fetch {
                    map.insert(sha.clone(), read_git_notes(&repo, sha).unwrap_or_default());
                }
                map
            }
            StorageBackend::LinksToml => {
                let existing: Option<HashMap<String, Vec<TicketLink>>> =
                    caches().get(&tab_id).and_then(|c| c.toml_all.clone());
                let all = match existing {
                    Some(c) => c,
                    None => {
                        let workdir = repo_path(state, &tab_id)?;
                        let loaded =
                            read_all_toml_links(Path::new(&workdir)).map_err(|e| e.to_string())?;
                        if let Some(cache) = caches().get_mut(&tab_id) {
                            cache.toml_all = Some(loaded.clone());
                        }
                        loaded
                    }
                };
                let mut map = HashMap::new();
                for sha in &need_manual_fetch {
                    map.insert(sha.clone(), all.get(sha).cloned().unwrap_or_default());
                }
                map
            }
        };

        // ── Stage 3: store fetched data in cache, merge into result ───────────
        let mut c = caches();
        let cache = c.entry(tab_id).or_default();
        for (sha, manual_links) in fetched {
            cache.manual_checked.insert(sha.clone());
            if !manual_links.is_empty() {
                cache.manual.insert(sha.clone(), manual_links.clone());
            }
            if let Some(entry) = result.get_mut(&sha) {
                for link in manual_links {
                    if !entry.iter().any(|e| e.ticket_id == link.ticket_id) {
                        entry.push(link);
                    }
                }
            }
        }
    }

    Ok(result)
}

/// Manually link a commit to a ticket (persisted in the configured store).
#[arbor_rpc::handler]
fn add_ticket_link(
    state: &CorvusState,
    tab_id: String,
    sha: String,
    ticket_id: String,
    tracker: String,
) -> Result<(), String> {
    let config = effective_config(state, &tab_id)?;
    let workdir = repo_path(state, &tab_id)?;

    match &config.storage {
        StorageBackend::GitNotes => {
            let repo = open(state, &tab_id)?;
            let mut links = read_git_notes(&repo, &sha).map_err(|e| e.to_string())?;
            if !links.iter().any(|l| l.ticket_id == ticket_id) {
                links.push(TicketLink { ticket_id: ticket_id.clone(), tracker, source: LinkSource::Manual });
                write_git_notes(&repo, &sha, &links).map_err(|e| e.to_string())?;
            }
        }
        StorageBackend::LinksToml => {
            add_toml_link(Path::new(&workdir), &sha, &ticket_id, &tracker).map_err(|e| e.to_string())?;
        }
    }

    if let Some(cache) = caches().get_mut(&tab_id) {
        cache.invalidate_manual(&sha);
    }
    Ok(())
}

/// Remove a previously linked ticket from a commit.
#[arbor_rpc::handler]
fn remove_ticket_link(
    state: &CorvusState,
    tab_id: String,
    sha: String,
    ticket_id: String,
) -> Result<(), String> {
    let config = effective_config(state, &tab_id)?;
    let workdir = repo_path(state, &tab_id)?;

    match &config.storage {
        StorageBackend::GitNotes => {
            let repo = open(state, &tab_id)?;
            let mut links = read_git_notes(&repo, &sha).map_err(|e| e.to_string())?;
            links.retain(|l| l.ticket_id != ticket_id);
            write_git_notes(&repo, &sha, &links).map_err(|e| e.to_string())?;
        }
        StorageBackend::LinksToml => {
            remove_toml_link(Path::new(&workdir), &sha, &ticket_id).map_err(|e| e.to_string())?;
        }
    }

    if let Some(cache) = caches().get_mut(&tab_id) {
        cache.invalidate_manual(&sha);
    }
    Ok(())
}

/// Return the effective (merged global + per-repo) ticket-link config for a tab.
#[arbor_rpc::handler]
fn get_ticket_link_config(state: &CorvusState, tab_id: String) -> Result<TicketLinkConfig, String> {
    effective_config(state, &tab_id)
}

/// Persist per-repo ticket-link overrides to `.arbor/config.toml`, merging the
/// `[ticket_links]` table in while preserving every other section the file holds.
#[arbor_rpc::handler]
fn set_ticket_link_repo_config(
    state: &CorvusState,
    tab_id: String,
    config: TicketLinksRepoConfig,
) -> Result<(), String> {
    let workdir = repo_path(state, &tab_id)?;
    let path = Path::new(&workdir).join(".arbor").join("config.toml");

    // Read the existing file as a raw TOML table so sections corvus-be doesn't
    // model (gitflow, stats_exclude, local_tags, …) survive the write untouched.
    let mut table: toml::Table = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
    table.insert(
        "ticket_links".to_string(),
        toml::Value::try_from(&config).map_err(|e| e.to_string())?,
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = toml::to_string_pretty(&table).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;

    // Invalidate this tab's cache: the storage backend / tracker may have changed.
    if let Some(cache) = caches().get_mut(&tab_id) {
        cache.invalidate_all_manual();
        cache.auto_parsed.clear();
    }
    Ok(())
}

/// Validate a custom ticket regex pattern. Empty string = valid (compilable +
/// ≥1 capture group), else a human-readable error.
#[arbor_rpc::handler]
fn validate_ticket_regex(_state: &CorvusState, pattern: String) -> Result<String, String> {
    if pattern.trim().is_empty() {
        return Ok(String::new());
    }
    Ok(match regex::Regex::new(&pattern) {
        Err(e) => e.to_string(),
        Ok(re) if re.captures_len() <= 1 => {
            "Pattern must contain at least one capture group, e.g. \\b(PROJ-\\d+)\\b".to_string()
        }
        Ok(_) => String::new(),
    })
}

/// `true` if the repo's remote config already includes a push/fetch refspec for
/// `refs/notes/arbor/tickets` (else notes stay local — the FE warns the user).
#[arbor_rpc::handler]
fn check_notes_push_config(state: &CorvusState, tab_id: String) -> Result<bool, String> {
    let repo = open(state, &tab_id)?;
    Ok(check_notes_push_refspec(&repo))
}

/// Return all commits linked to `ticket_id` for the given tab, newest-first.
#[arbor_rpc::handler]
fn find_commits_for_ticket(
    state: &CorvusState,
    tab_id: String,
    ticket_id: String,
) -> Result<Vec<LinkedCommitRef>, String> {
    if !global_config(state).enabled {
        return Ok(vec![]);
    }

    let config = effective_config(state, &tab_id)?;
    let workdir = repo_path(state, &tab_id)?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut result: Vec<LinkedCommitRef> = vec![];

    // ── Stage 1a: GitNotes full scan ──────────────────────────────────────────
    if matches!(config.storage, StorageBackend::GitNotes) {
        let repo = open(state, &tab_id)?;

        // Collect annotated OIDs first so the Notes iterator drops before we
        // borrow `repo` again for find_note / find_commit.
        let annotated_oids: Vec<git2::Oid> = repo
            .notes(Some(NOTES_REF))
            .map(|iter| iter.flatten().map(|(_, annotated)| annotated).collect())
            .unwrap_or_default();

        for annotated_oid in annotated_oids {
            let sha = annotated_oid.to_string();
            let links = read_git_notes(&repo, &sha).unwrap_or_default();
            if links.iter().any(|l| l.ticket_id == ticket_id) && seen.insert(sha.clone()) {
                if let Ok(commit) = repo.find_commit(annotated_oid) {
                    result.push(LinkedCommitRef {
                        short_oid: sha[..8.min(sha.len())].to_string(),
                        summary: commit.summary().unwrap_or("").to_string(),
                        author_name: commit.author().name().unwrap_or("").to_string(),
                        timestamp: commit.time().seconds(),
                        source: LinkSource::Manual,
                        sha,
                    });
                }
            }
        }
    }

    // ── Stage 1b: LinksToml — cache or load from disk ─────────────────────────
    let toml_shas: Vec<String> = if matches!(config.storage, StorageBackend::LinksToml) {
        let cached = caches().get(&tab_id).and_then(|c| c.toml_all.clone());
        let all = match cached {
            Some(c) => c,
            None => {
                let loaded = read_all_toml_links(Path::new(&workdir)).map_err(|e| e.to_string())?;
                if let Some(cache) = caches().get_mut(&tab_id) {
                    cache.toml_all = Some(loaded.clone());
                }
                loaded
            }
        };
        all.iter()
            .filter(|(_, links)| links.iter().any(|l| l.ticket_id == ticket_id))
            .map(|(sha, _)| sha.clone())
            .collect()
    } else {
        vec![]
    };

    // ── Stage 2: auto-parsed cache (partial — only visited commits) ───────────
    let auto_shas: Vec<(String, LinkSource)> = caches()
        .get(&tab_id)
        .map(|cache| {
            cache
                .auto_parsed
                .iter()
                .filter_map(|(sha, links)| {
                    links
                        .iter()
                        .find(|l| l.ticket_id == ticket_id)
                        .map(|l| (sha.clone(), l.source.clone()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // ── Stage 3: fetch commit metadata for toml + auto SHAs (one repo open) ───
    let mut need_detail: Vec<(String, LinkSource)> = vec![];
    for sha in toml_shas {
        if seen.insert(sha.clone()) {
            need_detail.push((sha, LinkSource::Manual));
        }
    }
    for (sha, src) in auto_shas {
        if seen.insert(sha.clone()) {
            need_detail.push((sha, src));
        }
    }

    if !need_detail.is_empty() {
        let repo = open(state, &tab_id)?;
        for (sha, source) in need_detail {
            if let Ok(oid) = git2::Oid::from_str(&sha) {
                if let Ok(commit) = repo.find_commit(oid) {
                    result.push(LinkedCommitRef {
                        short_oid: sha[..8.min(sha.len())].to_string(),
                        summary: commit.summary().unwrap_or("").to_string(),
                        author_name: commit.author().name().unwrap_or("").to_string(),
                        timestamp: commit.time().seconds(),
                        source,
                        sha,
                    });
                }
            }
        }
    }

    result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(result)
}
