//! Top-level `index.json` shape and the public `fetch_catalog` entry point.
//!
//! `index.json` lives at the root of any registry-shaped GitHub repo and
//! lists plugin + theme pointers. Each pointer is either internal (the
//! entry lives in the registry repo, under `subpath`) or external (it
//! points at a third-party repo, with an optional `pinned_sha`). Both
//! shapes resolve to `MarketplaceSource::Community` when fetched from the
//! curated registry — vetting happens via PR review on the registry side.

use futures_util::future::join_all;
use serde::Deserialize;

use crate::error::{MarketplaceError, Result};
use crate::fetch::{fetch_plugin, fetch_theme};
use crate::github_api::{
    github_url, normalise_github_url, parse_github_repo, raw_url, verify_pinned_sha,
};
use crate::types::{MarketplaceCatalog, MarketplacePlugin, MarketplaceSource, MarketplaceTheme};

/// Curated registry — flipped here so adding a mirror later is a one-line
/// change. Custom user sources pass their own URL to [`fetch_catalog`].
pub const REGISTRY_REPO: &str = "https://github.com/nightprint-studio/arbor-extensions";

/// We pin to `main` per design decision — tag-based resolution will land
/// once `arbor-extensions` has its first tagged release.
pub const REGISTRY_REF: &str = "main";

/// Hard cap on entries per `index.json`. By construction no entry triggers
/// further index fetches (External entries resolve to a single `plugin.toml`
/// leaf, not another catalog), so the worst-case fan-out from one registry
/// fetch is bounded by `2 * MAX_ENTRIES_PER_INDEX` HTTP requests (plugins +
/// themes, in parallel). The cap is here as a defence against a degenerate
/// or malicious index file slipping past PR review and exploding fetch
/// traffic — the live catalog has ~20 entries today.
const MAX_ENTRIES_PER_INDEX: usize = 1000;

// ---------------------------------------------------------------------------
// index.json shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct IndexFile {
    #[serde(default)] plugins: Vec<IndexEntry>,
    #[serde(default)] themes:  Vec<IndexEntry>,
}

/// An entry in the registry `index.json`. Two shapes, discriminated by the
/// presence of the `repo` field:
///
///   * **Internal** — `{ "subpath": "plugins/foo", "ref"?: "…" }` — the
///     plugin/theme lives inside the registry repo itself. This is the
///     original shape and matches every entry shipped today.
///   * **External** — `{ "repo": "https://github.com/owner/repo",
///     "subpath"?: "…", "ref"?: "…", "pinned_sha"?: "…" }` — the
///     plugin/theme lives in a third-party GitHub repo. The registry just
///     points at it. Both shapes resolve to `MarketplaceSource::Community`;
///     `pinned_sha` is the recommended-but-optional defence against
///     tag-hijack on third-party repos.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum IndexEntry {
    External {
        repo:                                          String,
        #[serde(default)]                              subpath:    Option<String>,
        #[serde(default)] #[serde(rename = "ref")]     r#ref:      Option<String>,
        #[serde(default)]                              pinned_sha: Option<String>,
    },
    Internal {
        subpath: String,
        #[serde(default)] #[serde(rename = "ref")]
        r#ref:   Option<String>,
    },
}

/// Resolved location an `IndexEntry` points at. Internal entries reuse the
/// host registry's `(owner, repo)`; external entries parse their own `repo`
/// URL. The downstream `fetch_*` calls take these primitives.
pub(crate) struct EntryTarget {
    pub owner:      String,
    pub repo:       String,
    pub subpath:    String,           // "" = root
    pub r#ref:      String,           // resolved (defaulted to REGISTRY_REF)
    pub pinned_sha: Option<String>,   // only ever Some for External entries
    pub external:   bool,             // mirrored onto RegistryEntry post-fetch
}

fn resolve_entry_target(
    entry:      &IndexEntry,
    host_owner: &str,
    host_repo:  &str,
) -> EntryTarget {
    match entry {
        IndexEntry::Internal { subpath, r#ref } => EntryTarget {
            owner:      host_owner.to_string(),
            repo:       host_repo.to_string(),
            subpath:    subpath.clone(),
            r#ref:      r#ref.clone().unwrap_or_else(|| REGISTRY_REF.to_string()),
            pinned_sha: None,
            external:   false,
        },
        IndexEntry::External { repo, subpath, r#ref, pinned_sha } => {
            // We tolerate a malformed `repo` field by surfacing it as
            // (entry-level) owner="" repo=""; the fetch call that follows
            // will fail with a clean HTTP error and the entry will be
            // logged + skipped. That keeps error reporting in one place.
            let (owner, repo_name) = parse_github_repo(repo)
                .unwrap_or_else(|| (String::new(), String::new()));
            EntryTarget {
                owner,
                repo:       repo_name,
                subpath:    subpath.clone().unwrap_or_default(),
                r#ref:      r#ref.clone().unwrap_or_else(|| REGISTRY_REF.to_string()),
                pinned_sha: pinned_sha.clone(),
                external:   true,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public fetch entry point
// ---------------------------------------------------------------------------

/// Fetch the catalog from a GitHub-hosted registry. Per-entry failures are
/// logged and dropped so a single bad submission doesn't blank the catalog.
pub async fn fetch_catalog(
    http:        &reqwest::Client,
    repo_url:    &str,
    source_kind: MarketplaceSource,
) -> Result<MarketplaceCatalog> {
    let (owner, repo) = parse_github_repo(repo_url)
        .ok_or_else(|| MarketplaceError::InvalidUrl(repo_url.to_string()))?;

    let index_url = raw_url(&owner, &repo, REGISTRY_REF, "index.json");
    tracing::info!("marketplace: fetching index from {index_url}");

    let index: IndexFile = http
        .get(&index_url)
        .send().await
        .map_err(|e| MarketplaceError::Other(format!("fetch {index_url}: {e}")))?
        .error_for_status()
        .map_err(|e| MarketplaceError::Other(format!("HTTP {index_url}: {e}")))?
        .json().await
        .map_err(|e| MarketplaceError::Other(format!("parse {index_url}: {e}")))?;

    // Defensive cap — see MAX_ENTRIES_PER_INDEX. We refuse the whole fetch
    // rather than truncating: a registry this large is almost certainly
    // broken, and partial results would silently hide entries the user
    // expects to see.
    if index.plugins.len() > MAX_ENTRIES_PER_INDEX
        || index.themes.len() > MAX_ENTRIES_PER_INDEX
    {
        return Err(MarketplaceError::Other(format!(
            "index.json at {index_url} exceeds the {MAX_ENTRIES_PER_INDEX}-entry cap \
             (plugins={}, themes={})",
            index.plugins.len(), index.themes.len(),
        )));
    }

    // Drop entries whose External `repo` points back at this very registry —
    // that's a manifest-authoring mistake (use Internal instead), not a
    // cycle (External is a leaf), but the resulting double-fetch is wasted
    // work and the entry would shadow itself in the catalog.
    let host_url_lc = github_url(&owner, &repo).to_lowercase();
    let points_at_host = |entry: &IndexEntry| matches!(entry,
        IndexEntry::External { repo, .. }
            if normalise_github_url(repo)
                .map(|u| u.to_lowercase() == host_url_lc)
                .unwrap_or(false)
    );
    let keep_entry = |kind: &str, e: IndexEntry| -> Option<IndexEntry> {
        if points_at_host(&e) {
            tracing::warn!(
                "marketplace: skipping External {kind} entry that points at the \
                 registry itself ({owner}/{repo}) — use the Internal shape \
                 (just `subpath`) for entries hosted in the registry repo"
            );
            None
        } else { Some(e) }
    };
    let plugin_entries: Vec<_> = index.plugins.into_iter()
        .filter_map(|e| keep_entry("plugin", e)).collect();
    let theme_entries: Vec<_> = index.themes.into_iter()
        .filter_map(|e| keep_entry("theme",  e)).collect();

    // Plugins — fetched in parallel.
    let plugin_futs = plugin_entries.iter().cloned().map(|entry| {
        let http       = http.clone();
        let host_owner = owner.clone();
        let host_repo  = repo.clone();
        let src        = source_kind;
        async move {
            let t = resolve_entry_target(&entry, &host_owner, &host_repo);
            let mut p = fetch_plugin(&http, &t.owner, &t.repo, &t.r#ref, &t.subpath, src).await?;
            p.entry.external = t.external;
            if let Some(pin) = t.pinned_sha.as_deref() {
                verify_pinned_sha(&http, &t.owner, &t.repo, &t.r#ref, pin).await?;
                p.entry.pinned_sha = Some(pin.to_string());
            }
            Ok::<_, MarketplaceError>(p)
        }
    });
    let theme_futs = theme_entries.iter().cloned().map(|entry| {
        let http       = http.clone();
        let host_owner = owner.clone();
        let host_repo  = repo.clone();
        let src        = source_kind;
        async move {
            let t = resolve_entry_target(&entry, &host_owner, &host_repo);
            if t.subpath.is_empty() {
                return Err(MarketplaceError::Other(
                    "theme entry has no subpath (need the .json filename)".into(),
                ));
            }
            let mut th = fetch_theme(&http, &t.owner, &t.repo, &t.r#ref, &t.subpath, src).await?;
            th.entry.external = t.external;
            if let Some(pin) = t.pinned_sha.as_deref() {
                verify_pinned_sha(&http, &t.owner, &t.repo, &t.r#ref, pin).await?;
                th.entry.pinned_sha = Some(pin.to_string());
            }
            Ok::<_, MarketplaceError>(th)
        }
    });

    let (plugin_results, theme_results) = tokio::join!(
        join_all(plugin_futs),
        join_all(theme_futs),
    );

    let mut plugins: Vec<MarketplacePlugin> = plugin_results.into_iter()
        .filter_map(|r| match r {
            Ok(p)  => Some(p),
            Err(e) => { tracing::warn!("marketplace plugin entry skipped: {e}"); None }
        })
        .collect();
    let mut themes: Vec<MarketplaceTheme> = theme_results.into_iter()
        .filter_map(|r| match r {
            Ok(t)  => Some(t),
            Err(e) => { tracing::warn!("marketplace theme entry skipped: {e}"); None }
        })
        .collect();

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    themes .sort_by(|a, b| a.name.cmp(&b.name));

    Ok(MarketplaceCatalog { plugins, themes })
}
