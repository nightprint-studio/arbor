use std::collections::{HashMap, HashSet};
use std::path::Path;

use tauri::State;

use crate::AppState;
use crate::config::repo_config;
use crate::config::repo_config::TicketLinksRepoConfig;
use crate::error::Result;
use crate::git::ticket_links::{
    LinkSource, StorageBackend, TicketLink, TicketLinkConfig,
    add_toml_link, check_notes_push_refspec, parse_text,
    read_all_toml_links, read_git_notes, remove_toml_link, write_git_notes,
    NOTES_REF,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Quickly obtain the repo working-directory path, releasing the repos mutex.
fn get_workdir(state: &State<'_, AppState>, tab_id: &str) -> Result<String> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(tab_id)?;
    Ok(repo.inner()
        .workdir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default())
}

/// Resolve the effective ticket-link config for `tab_id`:
/// global defaults ← overridden by per-repo `.arbor/config.toml`.
fn effective_config(state: &State<'_, AppState>, tab_id: &str) -> Result<TicketLinkConfig> {
    // 1. Read global config (release lock immediately).
    let global = {
        let cfg = state.lock_config()?;
        cfg.ticket_links.clone()
    };

    // 2. Read per-repo config (release lock immediately).
    let workdir = get_workdir(state, tab_id)?;
    let repo_cfg = repo_config::load(&workdir).unwrap_or_default();

    let storage = repo_cfg.ticket_links.as_ref()
        .and_then(|c| c.storage.clone())
        .unwrap_or(global.storage);

    // tracker: repo.ticket_links.tracker > repo.issue_tracker (legacy) > None
    let tracker = repo_cfg.ticket_links.as_ref()
        .and_then(|c| c.tracker.clone())
        .or_else(|| repo_cfg.issue_tracker.clone());

    let auto_parse = repo_cfg.ticket_links.as_ref()
        .and_then(|c| c.auto_parse)
        .unwrap_or(global.auto_parse);

    let custom_pattern = repo_cfg.ticket_links.as_ref()
        .and_then(|c| c.custom_pattern.clone());

    Ok(TicketLinkConfig { storage, tracker, auto_parse, warn_push: global.warn_push, custom_pattern })
}

// ── Input DTO ─────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CommitQueryItem {
    pub sha:     String,
    pub message: String,
    /// Branch/tag names that point at (or are ancestors of) this commit.
    pub refs:    Vec<String>,
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Batch-fetch ticket links for a list of visible commits.
/// Returns a map of SHA → links (empty vec when none found).
/// Only SHAs not already in the in-memory cache trigger I/O.
#[tauri::command]
pub fn get_commit_ticket_links(
    state: State<'_, AppState>,
    tab_id: String,
    commits: Vec<CommitQueryItem>,
) -> Result<HashMap<String, Vec<TicketLink>>> {
    // Fast-exit when the feature is globally disabled.
    if !state.lock_config()?.ticket_links.enabled {
        return Ok(HashMap::new());
    }

    let config = effective_config(&state, &tab_id)?;

    // Compile the custom regex once for the whole batch (cheap: only when set).
    // `captures_len()` includes the whole-match slot (index 0), so >1 means ≥1 capture group.
    let custom_compiled: Option<regex::Regex> = config.custom_pattern.as_deref()
        .and_then(|p| regex::Regex::new(p).ok())
        .filter(|re| re.captures_len() > 1);
    let custom_re: Option<&regex::Regex> = custom_compiled.as_ref();

    // ── Stage 1: check cache, build preliminary result, collect uncached SHAs ──
    let (mut result, need_manual_fetch): (HashMap<String, Vec<TicketLink>>, Vec<String>) = {
        let mut caches = state.lock_ticket_caches()?;
        let cache = caches.entry(tab_id.clone()).or_default();

        let mut res: HashMap<String, Vec<TicketLink>> = HashMap::new();
        let mut need_fetch: Vec<String> = vec![];

        for item in &commits {
            let mut links: Vec<TicketLink> = vec![];

            // Auto-parse (commit messages + branch names — immutable, cached forever).
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

            // Manual links from storage.
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
    }; // caches lock released here

    // ── Stage 2: fetch uncached manual links from the backing store ────────────
    if !need_manual_fetch.is_empty() {
        let fetched: HashMap<String, Vec<TicketLink>> = match &config.storage {
            StorageBackend::GitNotes => {
                let mut repos = state.lock_repos()?;
                let repo  = repos.get(&tab_id)?;
                let inner = repo.inner();
                let mut map = HashMap::new();
                for sha in &need_manual_fetch {
                    map.insert(sha.clone(), read_git_notes(inner, sha).unwrap_or_default());
                }
                map
                // repos lock released here
            }
            StorageBackend::LinksToml => {
                // Check if the full TOML map is already in cache.
                let existing: Option<HashMap<String, Vec<TicketLink>>> = {
                    let caches = state.lock_ticket_caches()?;
                    caches.get(&tab_id).and_then(|c| c.toml_all.clone())
                };
                let all = match existing {
                    Some(c) => c,
                    None => {
                        let workdir = get_workdir(&state, &tab_id)?;
                        let loaded = read_all_toml_links(std::path::Path::new(&workdir))?;
                        // Cache the whole file for subsequent calls.
                        {
                            let mut caches = state.lock_ticket_caches()?;
                            if let Some(cache) = caches.get_mut(&tab_id) {
                                cache.toml_all = Some(loaded.clone());
                            }
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
        let mut caches = state.lock_ticket_caches()?;
        let cache = caches.entry(tab_id).or_default();
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
#[tauri::command]
pub fn add_ticket_link(
    state:     State<'_, AppState>,
    tab_id:    String,
    sha:       String,
    ticket_id: String,
    tracker:   String,
) -> Result<()> {
    let config = effective_config(&state, &tab_id)?;
    let workdir = get_workdir(&state, &tab_id)?;

    match &config.storage {
        StorageBackend::GitNotes => {
            let mut repos = state.lock_repos()?;
            let repo  = repos.get(&tab_id)?;
            let mut links = read_git_notes(repo.inner(), &sha)?;
            if !links.iter().any(|l| l.ticket_id == ticket_id) {
                links.push(TicketLink { ticket_id: ticket_id.clone(), tracker, source: LinkSource::Manual });
                write_git_notes(repo.inner(), &sha, &links)?;
            }
        }
        StorageBackend::LinksToml => {
            add_toml_link(std::path::Path::new(&workdir), &sha, &ticket_id, &tracker)?;
        }
    }

    // Invalidate cache for this SHA.
    let mut caches = state.lock_ticket_caches()?;
    if let Some(cache) = caches.get_mut(&tab_id) {
        cache.invalidate_manual(&sha);
    }
    Ok(())
}

/// Remove a previously linked ticket from a commit.
#[tauri::command]
pub fn remove_ticket_link(
    state:     State<'_, AppState>,
    tab_id:    String,
    sha:       String,
    ticket_id: String,
) -> Result<()> {
    let config = effective_config(&state, &tab_id)?;
    let workdir = get_workdir(&state, &tab_id)?;

    match &config.storage {
        StorageBackend::GitNotes => {
            let mut repos = state.lock_repos()?;
            let repo  = repos.get(&tab_id)?;
            let mut links = read_git_notes(repo.inner(), &sha)?;
            links.retain(|l| l.ticket_id != ticket_id);
            write_git_notes(repo.inner(), &sha, &links)?;
        }
        StorageBackend::LinksToml => {
            remove_toml_link(std::path::Path::new(&workdir), &sha, &ticket_id)?;
        }
    }

    let mut caches = state.lock_ticket_caches()?;
    if let Some(cache) = caches.get_mut(&tab_id) {
        cache.invalidate_manual(&sha);
    }
    Ok(())
}

/// Return the effective (merged global + per-repo) ticket-link config for a tab.
#[tauri::command]
pub fn get_ticket_link_config(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<TicketLinkConfig> {
    effective_config(&state, &tab_id)
}

/// Persist per-repo ticket-link overrides to `.arbor/config.toml`.
#[tauri::command]
pub fn set_ticket_link_repo_config(
    state:  State<'_, AppState>,
    tab_id: String,
    config: TicketLinksRepoConfig,
) -> Result<()> {
    let workdir = get_workdir(&state, &tab_id)?;
    let mut repo_cfg = repo_config::load(&workdir).unwrap_or_default();
    repo_cfg.ticket_links = Some(config);
    repo_config::save(&workdir, &repo_cfg)?;
    // Invalidate all manual caches for this tab (storage backend may have changed).
    let mut caches = state.lock_ticket_caches()?;
    if let Some(cache) = caches.get_mut(&tab_id) {
        cache.invalidate_all_manual();
        cache.auto_parsed.clear(); // tracker may have changed too
    }
    Ok(())
}

/// Validate a custom ticket regex pattern.
/// Returns an empty string when valid (compilable + has ≥1 capture group),
/// or a human-readable error message when not.
#[tauri::command]
pub fn validate_ticket_regex(pattern: String) -> String {
    if pattern.trim().is_empty() {
        return String::new();
    }
    match regex::Regex::new(&pattern) {
        Err(e) => e.to_string(),
        Ok(re) if re.captures_len() <= 1 => {
            "Pattern must contain at least one capture group, e.g. \\b(PROJ-\\d+)\\b".to_string()
        }
        Ok(_) => String::new(),
    }
}

/// Returns `true` if the repo's remote config already includes a push/fetch
/// refspec for `refs/notes/arbor/tickets`.  When `false`, notes are local-only
/// and will NOT be pushed to the remote — the frontend should warn the user.
#[tauri::command]
pub fn check_notes_push_config(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<bool> {
    let mut repos = state.lock_repos()?;
    let repo  = repos.get(&tab_id)?;
    Ok(check_notes_push_refspec(repo.inner()))
}

// ── Reverse lookup: ticket → commits ──────────────────────────────────────────

/// A commit that is associated (manually or via auto-parse) with a ticket.
#[derive(serde::Serialize)]
pub struct LinkedCommitRef {
    pub sha:         String,
    pub short_oid:   String,
    pub summary:     String,
    pub author_name: String,
    pub timestamp:   i64,
    /// How the association was discovered.
    pub source:      LinkSource,
}

/// Return all commits linked to `ticket_id` for the given tab.
///
/// - Manual links: full scan of the configured storage backend (git notes or
///   links.toml).  links.toml is served from the in-memory cache when warm.
/// - Auto-parsed links: served from the in-memory `auto_parsed` cache
///   (covers commits already scrolled into view — not exhaustive).
///
/// Results are sorted newest-first.
#[tauri::command]
pub fn find_commits_for_ticket(
    state:     State<'_, AppState>,
    tab_id:    String,
    ticket_id: String,
) -> Result<Vec<LinkedCommitRef>> {
    if !state.lock_config()?.ticket_links.enabled {
        return Ok(vec![]);
    }

    let config  = effective_config(&state, &tab_id)?;
    let workdir = get_workdir(&state, &tab_id)?;

    let mut seen:   HashSet<String>        = HashSet::new();
    let mut result: Vec<LinkedCommitRef>   = vec![];

    // ── Stage 1a: GitNotes full scan (repo lock covers scan + commit lookup) ──
    if matches!(config.storage, StorageBackend::GitNotes) {
        let mut repos = state.lock_repos()?;
        let repo  = repos.get(&tab_id)?;
        let inner = repo.inner();

        // Collect annotated OIDs first so the Notes iterator drops before we
        // borrow inner again for find_note / find_commit.
        let annotated_oids: Vec<git2::Oid> = inner
            .notes(Some(NOTES_REF))
            .map(|iter| {
                iter.flatten()
                    .map(|(_, annotated)| annotated)
                    .collect()
            })
            .unwrap_or_default();

        for annotated_oid in annotated_oids {
            let sha = annotated_oid.to_string();
            let links = read_git_notes(inner, &sha).unwrap_or_default();
            if links.iter().any(|l| l.ticket_id == ticket_id) {
                if seen.insert(sha.clone()) {
                    if let Ok(commit) = inner.find_commit(annotated_oid) {
                        result.push(LinkedCommitRef {
                            short_oid:   sha[..8.min(sha.len())].to_string(),
                            summary:     commit.summary().unwrap_or("").to_string(),
                            author_name: commit.author().name().unwrap_or("").to_string(),
                            timestamp:   commit.time().seconds(),
                            source:      LinkSource::Manual,
                            sha,
                        });
                    }
                }
            }
        }
        // repos lock released here
    }

    // ── Stage 1b: LinksToml — use cache or load from disk ────────────────────
    let toml_shas: Vec<String> = if matches!(config.storage, StorageBackend::LinksToml) {
        let cached = {
            let caches = state.lock_ticket_caches()?;
            caches.get(&tab_id).and_then(|c| c.toml_all.clone())
        };
        let all = match cached {
            Some(c) => c,
            None => {
                let loaded = read_all_toml_links(Path::new(&workdir))?;
                let mut caches = state.lock_ticket_caches()?;
                if let Some(cache) = caches.get_mut(&tab_id) {
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
    let auto_shas: Vec<(String, LinkSource)> = {
        let caches = state.lock_ticket_caches()?;
        caches
            .get(&tab_id)
            .map(|cache| {
                cache
                    .auto_parsed
                    .iter()
                    .filter_map(|(sha, links)| {
                        links.iter().find(|l| l.ticket_id == ticket_id).map(|l| {
                            (sha.clone(), l.source.clone())
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    // ── Stage 3: fetch commit metadata for toml + auto SHAs (one repo lock) ──
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
        let mut repos = state.lock_repos()?;
        let repo  = repos.get(&tab_id)?;
        let inner = repo.inner();
        for (sha, source) in need_detail {
            if let Ok(oid) = git2::Oid::from_str(&sha) {
                if let Ok(commit) = inner.find_commit(oid) {
                    result.push(LinkedCommitRef {
                        short_oid:   sha[..8.min(sha.len())].to_string(),
                        summary:     commit.summary().unwrap_or("").to_string(),
                        author_name: commit.author().name().unwrap_or("").to_string(),
                        timestamp:   commit.time().seconds(),
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
