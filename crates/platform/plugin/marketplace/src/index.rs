//! Top-level `index.json` shape and the public `fetch_catalog` entry point.
//!
//! `index.json` lives at the root of any registry-shaped GitHub repo and
//! lists plugin + theme pointers. Each pointer is either internal (the
//! entry lives in the registry repo, under `subpath`) or external (it
//! points at a third-party repo, with an optional `pinned_sha`). Both
//! shapes resolve to `MarketplaceSource::Community` when fetched from the
//! curated registry — vetting happens via PR review on the registry side.

use std::collections::BTreeMap;

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

/// The branch the registry's own `index.json` is read from, and the fallback for an entry
/// that names no `ref`.
///
/// **An entry that rides this is installing whatever is on HEAD.** That is not a version — it
/// is "whatever was pushed most recently", which means two users installing the same package
/// on the same day can get different code and neither can say which. It is the state the
/// catalogue has always been in, and it stays supported because the twenty packages listed
/// today rely on it; what changed is that it no longer happens quietly (see
/// [`unpinned_entries`]).
pub const REGISTRY_REF: &str = "main";

/// Entries that resolve against a moving branch rather than a tag.
///
/// Reported rather than refused: refusing would empty the catalogue, and the twenty packages
/// listed today all look like this. What it buys is that "this package has no version" becomes
/// something the log says once per fetch instead of something nobody can see — which is the
/// precondition for fixing it entry by entry.
pub fn unpinned_entries(catalog: &MarketplaceCatalog) -> Vec<String> {
    catalog
        .plugins
        .iter()
        .filter(|p| p.entry.r#ref.as_deref().is_none_or(|r| r == REGISTRY_REF))
        .map(|p| p.name.clone())
        .collect()
}

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
        #[serde(default)]                              artifacts:  BTreeMap<String, String>,
    },
    Internal {
        subpath: String,
        #[serde(default)] #[serde(rename = "ref")]
        r#ref:   Option<String>,
        #[serde(default)]
        artifacts: BTreeMap<String, String>,
    },
}

/// Resolved location an `IndexEntry` points at. Internal entries reuse the
/// host registry's `(owner, repo)`; external entries parse their own `repo`
/// URL. The downstream `fetch_*` calls take these primitives.
#[derive(Debug)]
pub(crate) struct EntryTarget {
    pub owner:      String,
    pub repo:       String,
    pub subpath:    String,           // "" = root
    pub r#ref:      String,           // resolved (defaulted to REGISTRY_REF)
    pub pinned_sha: Option<String>,   // only ever Some for External entries
    pub external:   bool,             // mirrored onto RegistryEntry post-fetch
    /// Digests of the release assets this entry approves. Empty for a source-archive
    /// install; see [`crate::integrity`] for why the two have different integrity stories.
    pub artifacts:  BTreeMap<String, String>,
}

/// An entry that approves artifact digests must name the exact ref they belong to.
///
/// The failure this prevents is quiet and confusing: digests pinned to a moving branch match
/// until the branch moves, and then every install of a package that was never touched starts
/// failing an integrity check. A release belongs to a tag by construction, so requiring one
/// costs an author nothing and removes the whole class.
fn check_ref_is_explicit(
    artifacts: &BTreeMap<String, String>,
    r#ref:     &Option<String>,
    subpath:   &str,
) -> Result<()> {
    if artifacts.is_empty() || r#ref.is_some() {
        return Ok(());
    }
    Err(MarketplaceError::InvalidEntry(format!(
        "'{subpath}' records artifact digests but no `ref`. Digests pin exact bytes, so the          entry has to pin the exact release they came from — set `ref` to the tag."
    )))
}

fn resolve_entry_target(
    entry:      &IndexEntry,
    host_owner: &str,
    host_repo:  &str,
) -> Result<EntryTarget> {
    match entry {
        IndexEntry::Internal { subpath, r#ref, artifacts } => {
            check_ref_is_explicit(artifacts, r#ref, subpath)?;
            Ok(EntryTarget {
                owner:      host_owner.to_string(),
                repo:       host_repo.to_string(),
                subpath:    subpath.clone(),
                r#ref:      r#ref.clone().unwrap_or_else(|| REGISTRY_REF.to_string()),
                pinned_sha: None,
                external:   false,
                artifacts:  artifacts.clone(),
            })
        }
        IndexEntry::External { repo, subpath, r#ref, pinned_sha, artifacts } => {
            check_ref_is_explicit(artifacts, r#ref, subpath.as_deref().unwrap_or(repo))?;
            // We tolerate a malformed `repo` field by surfacing it as
            // (entry-level) owner="" repo=""; the fetch call that follows
            // will fail with a clean HTTP error and the entry will be
            // logged + skipped. That keeps error reporting in one place.
            let (owner, repo_name) = parse_github_repo(repo)
                .unwrap_or_else(|| (String::new(), String::new()));
            Ok(EntryTarget {
                owner,
                repo:       repo_name,
                subpath:    subpath.clone().unwrap_or_default(),
                r#ref:      r#ref.clone().unwrap_or_else(|| REGISTRY_REF.to_string()),
                pinned_sha: pinned_sha.clone(),
                external:   true,
                artifacts:  artifacts.clone(),
            })
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
    //
    // Source promotion: inside a curated-registry fetch (source_kind =
    // Community), Internal entries become `Official` (the plugin lives
    // in the registry repo itself, authored by the maintainers). External
    // entries stay `Community` — they're PR-vetted but the source is in
    // a third-party repo. Custom-source fetches don't get the promotion:
    // every entry from a user-added URL is uniformly `Custom`. We inline
    // the rule rather than capture an outer closure because `async move`
    // futures don't compose cleanly with closure captures.
    let plugin_futs = plugin_entries.iter().cloned().map(|entry| {
        let http       = http.clone();
        let host_owner = owner.clone();
        let host_repo  = repo.clone();
        let src_kind   = source_kind;
        async move {
            let t   = resolve_entry_target(&entry, &host_owner, &host_repo)?;
            let src = promote_source(src_kind, t.external);
            let mut p = fetch_plugin(&http, &t.owner, &t.repo, &t.r#ref, &t.subpath, src).await?;
            p.entry.external = t.external;
            // What the registry approved. Carried onto the resolved entry rather than read
            // from the package: the installer verifies against the review, not the author.
            p.entry.artifacts = t.artifacts.clone();
            // A package that provides something ships a module, and a module is a build
            // output that only travels as a release asset. An entry with no digests would
            // install it from the source archive, land a directory with no `.wasm` in it, and
            // fail later as a missing module — at which point nothing points back at the
            // registry entry that was wrong. Refused here, where the entry is in hand.
            if !p.provides.is_empty() && p.entry.artifacts.is_empty() {
                return Err(MarketplaceError::InvalidEntry(format!(
                    "'{}' provides {} interface(s) but its registry entry records no \
                     artifacts. A package that ships a module installs from its release: add \
                     `ref` and an `artifacts` map, or do not list it until it has one.",
                    p.name,
                    p.provides.len(),
                )));
            }
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
        let src_kind   = source_kind;
        async move {
            let t   = resolve_entry_target(&entry, &host_owner, &host_repo)?;
            let src = promote_source(src_kind, t.external);
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

    let catalog = MarketplaceCatalog { plugins, themes };
    let unpinned = unpinned_entries(&catalog);
    if !unpinned.is_empty() {
        tracing::warn!(
            "marketplace: {} entries have no `ref` and resolve against '{REGISTRY_REF}' — \
             installing them gets whatever is on HEAD rather than a version: {unpinned:?}",
            unpinned.len(),
        );
    }
    Ok(catalog)
}

// ---------------------------------------------------------------------------
// Source promotion (see fetch_catalog for the policy comment)
// ---------------------------------------------------------------------------

/// Apply the Community → Official promotion rule. Free function (not a
/// closure) so the rule composes cleanly with `async move` futures that
/// run concurrently — closure captures of an outer closure don't
/// auto-Copy in every Rust version we support and the resulting compile
/// surface is fragile.
fn promote_source(kind: MarketplaceSource, external: bool) -> MarketplaceSource {
    match (kind, external) {
        (MarketplaceSource::Community, false) => MarketplaceSource::Official,
        (other, _)                            => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(json: &str) -> IndexEntry {
        serde_json::from_str(json).expect("index entry")
    }

    #[test]
    fn the_shape_that_ships_today_still_parses() {
        // Every entry in the live `index.json` looks exactly like this. If it ever stops
        // parsing, the catalog goes blank for everyone at once.
        let t = resolve_entry_target(&entry(r#"{"subpath":"plugins/foo"}"#), "o", "r").unwrap();
        assert_eq!(t.subpath, "plugins/foo");
        assert_eq!(t.r#ref, REGISTRY_REF);
        assert!(!t.external);
        assert!(t.artifacts.is_empty());
    }

    #[test]
    fn an_external_entry_keeps_its_own_repo_and_pin() {
        let t = resolve_entry_target(
            &entry(r#"{"repo":"https://github.com/a/b","subpath":"p","pinned_sha":"9f2c1ab"}"#),
            "o",
            "r",
        )
        .unwrap();
        assert_eq!((t.owner.as_str(), t.repo.as_str()), ("a", "b"));
        assert_eq!(t.pinned_sha.as_deref(), Some("9f2c1ab"));
        assert!(t.external);
    }

    #[test]
    fn artifacts_ride_through_to_the_target() {
        let t = resolve_entry_target(
            &entry(
                r#"{"subpath":"packages/cloud-gcs","ref":"cloud-gcs-v1.4.0",
                    "artifacts":{"cloud_gcs.wasm":"sha256:1b7d"}}"#,
            ),
            "o",
            "r",
        )
        .unwrap();
        assert_eq!(t.r#ref, "cloud-gcs-v1.4.0");
        assert_eq!(t.artifacts.get("cloud_gcs.wasm").map(String::as_str), Some("sha256:1b7d"));
    }

    #[test]
    fn artifacts_without_an_explicit_ref_are_refused() {
        // The quiet failure this prevents: digests pinned to a branch match until the branch
        // moves, and then a package nobody touched starts failing an integrity check.
        let err = resolve_entry_target(
            &entry(r#"{"subpath":"packages/cloud-gcs","artifacts":{"x.wasm":"sha256:1b7d"}}"#),
            "o",
            "r",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("packages/cloud-gcs"), "{err}");
        assert!(err.contains("set `ref` to the tag"), "the error has to say the fix: {err}");
    }

    #[test]
    fn an_external_entry_with_artifacts_needs_a_ref_too() {
        assert!(resolve_entry_target(
            &entry(r#"{"repo":"https://github.com/a/b","artifacts":{"x.wasm":"sha256:1b7d"}}"#),
            "o",
            "r",
        )
        .is_err());
    }

    #[test]
    fn an_entry_riding_the_branch_is_reported_as_unpinned() {
        // Not refused — the twenty packages listed today all look like this — but no longer
        // silent. "This package has no version" has to be something the log says before it
        // can be something anybody fixes.
        use crate::types::{MarketplaceCatalog, MarketplacePlugin, MarketplaceSource, RegistryEntry};
        let mk = |name: &str, r: Option<&str>| MarketplacePlugin {
            name: name.into(), version: "1.0.0".into(), description: String::new(),
            author: String::new(), category: None, tags: None, repository: None,
            homepage: None, min_arbor_version: None, icon: None, screenshots: None,
            permissions: None, source: MarketplaceSource::Community, installed: false,
            enabled: None,
            entry: RegistryEntry {
                repo: String::new(), r#ref: r.map(str::to_string), subpath: None,
                source: MarketplaceSource::Community, pinned_sha: None, external: false,
                artifacts: Default::default(),
            },
            experimental: None, doc: None, update_available: None, installed_version: None,
            dependencies: vec![], credentials: vec![], provides: vec![],
        };
        let catalog = MarketplaceCatalog {
            plugins: vec![mk("rides-head", None), mk("also-head", Some("main")), mk("pinned", Some("v1.0.0"))],
            themes: vec![],
        };
        assert_eq!(unpinned_entries(&catalog), vec!["rides-head", "also-head"]);
    }

    #[test]
    fn an_entry_with_a_ref_and_no_artifacts_is_fine() {
        // Pinning a Lua plugin to a tag is allowed and always was — the new rule only runs
        // in the other direction.
        let t = resolve_entry_target(&entry(r#"{"subpath":"p","ref":"v1.0.0"}"#), "o", "r")
            .unwrap();
        assert_eq!(t.r#ref, "v1.0.0");
    }
}
