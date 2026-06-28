pub mod linear;
pub mod jira;
pub mod jira_types;
pub mod registry;

// Provider-agnostic DTOs (Issue, comments, filters, …) and pure helpers
// (`branch_name_for_issue`) live in the `corvus-issue-tracker-*` crates, bundled
// + re-exported by the shared `corvus-issues` crate. Re-exported here so existing
// `crate::integrations::Issue` / `crate::integrations::branch_name_for_issue`
// call sites keep working unchanged.
pub use corvus_issues::prelude::*;

// What stays here is host-coupled: it reads the repo config and dispatches to
// the per-provider modules (which themselves use the OS keyring + AppError).

/// Resolve the active issue tracker for a repo: per-repo `issue_tracker`
/// (with the legacy `ticket_links.tracker` override) — None if neither is set.
fn tracker_for_repo(repo_path: &str) -> Option<String> {
    // corvus-be owns RepoConfig; the shell only needs these two fields here, so
    // it reads them directly off the workdir (partial-read precedent — see
    // corvus-be's `stats_exclude_for`).
    #[derive(serde::Deserialize)]
    struct TrackerProbe {
        #[serde(default)]
        issue_tracker: Option<String>,
        #[serde(default)]
        ticket_links: Option<TicketLinksProbe>,
    }
    #[derive(serde::Deserialize)]
    struct TicketLinksProbe {
        #[serde(default)]
        tracker: Option<String>,
    }
    let path = std::path::Path::new(repo_path).join(".arbor").join("config.toml");
    let cfg: TrackerProbe = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())?;
    cfg.ticket_links.and_then(|c| c.tracker).or(cfg.issue_tracker)
}

/// Look up a single issue by its human identifier (e.g. `"ENG-42"`,
/// `"PROJ-123"`), routing to the tracker configured for the repo.
///
/// Returns:
///   · `Ok(Some(issue))` — found
///   · `Ok(None)`        — repo has no tracker configured, or no match
///   · `Err(_)`          — network / auth failure on the chosen tracker
///
/// Linear: performs `search_issues(query=identifier, limit=10)` and
/// filters by exact identifier match (Linear's search returns up to N
/// candidates whose number matches across teams). The first exact-match
/// row wins.
/// Jira: hands the key straight to `get_issue` — Jira's REST resolves
/// keys natively.
pub async fn lookup_by_identifier(
    repo_path:  &str,
    identifier: &str,
) -> crate::error::Result<Option<Issue>> {
    let id = identifier.trim();
    if id.is_empty() { return Ok(None); }
    let Some(tracker) = tracker_for_repo(repo_path) else { return Ok(None); };

    // Registered trackers (Linear + Jira) self-resolve via the trait — each
    // impl handles its own "missing key → None" semantics.
    match registry::registry().get(&tracker) {
        Some(t) => t.lookup_by_identifier(id).await.map_err(registry::to_app_error),
        None => Ok(None),
    }
}
