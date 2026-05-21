//! Plugin & theme marketplace.
//!
//! The marketplace catalog is a remote view (community + user-added
//! custom sources) PLUS a `Local` slice surfacing whatever lives on disk
//! that isn't tied to a marketplace entry — dev plugins, hand-copied
//! folders, themes dropped into `~/.config/arbor/themes/`.
//!
//! Important: locals are never *reconciled* with community/custom
//! entries by name. If a plugin lives in both `plugin_dir()` (dev) and
//! `marketplace_plugins/` (downloaded), the dev install wins on disk
//! (the host's `discover_plugins` skips the shadow) and we surface a
//! single Local entry for it — we don't overlay the community version's
//! metadata on top. That keeps the two pools conceptually separate
//! while still letting the user see *everything they have installed*
//! when they flip to the "Installed" tab.
//!
//! The marketplace's per-entry "installed" flag is read from:
//!   * `marketplace_installed.json` for community/custom downloads
//!   * the on-disk plugin manifest / theme JSON for Local entries

pub mod cache;
pub mod fetcher;
pub mod installer;
pub mod installs;
pub mod scheduler;
pub mod types;
pub mod user_registry;

use crate::error::Result;

use types::{
    MarketplaceCatalog, MarketplacePlugin, MarketplaceSource, MarketplaceTheme,
    RegistryEntry,
};
use user_registry::UserSource;

// ---------------------------------------------------------------------------
// In-memory registry
// ---------------------------------------------------------------------------

pub struct MarketplaceRegistry {
    /// Community catalog from the last successful fetch (or cache restore).
    community: MarketplaceCatalog,
    /// Repo URL we fetch the community catalog from. Override-able later
    /// (settings panel / staging / fork testing).
    community_repo: String,
    /// User-added custom plugin pointers. Phase 4 persists these.
    custom: Vec<MarketplacePlugin>,
}

impl MarketplaceRegistry {
    pub fn new() -> Self {
        let community = cache::load_any()
            .map(|f| f.catalog)
            .unwrap_or_default();
        // Cold-start fallback: prefer the last cached resolution of custom
        // sources so an offline boot still paints something. Refresh
        // replaces this whenever the user opens the modal.
        let custom = cache::load_custom();
        Self {
            community,
            community_repo: fetcher::REGISTRY_REPO.to_string(),
            custom,
        }
    }

    pub fn community_repo(&self) -> &str { &self.community_repo }

    /// True when we have a non-stale cache for the current community repo —
    /// lets the command layer skip a network hit on modal open.
    pub fn has_fresh_cache(&self) -> bool {
        cache::load_if_fresh(&self.community_repo).is_some()
    }

    /// Replace the community catalog after a successful fetch. Persists to
    /// the on-disk cache as a side effect.
    pub fn set_community(&mut self, catalog: MarketplaceCatalog) {
        cache::save(&self.community_repo, &catalog);
        self.community = catalog;
    }

    /// Replace the resolved custom plugins after a successful fetch. Cached
    /// to disk so subsequent boots have a starting point.
    pub fn set_custom(&mut self, plugins: Vec<MarketplacePlugin>) {
        cache::save_custom(&plugins);
        self.custom = plugins;
    }

    /// Look up a catalog entry by name across community + custom. Used by
    /// install command to find what to download.
    pub fn find_plugin(&self, name: &str) -> Option<MarketplacePlugin> {
        self.community.plugins.iter().find(|p| p.name == name)
            .or_else(|| self.custom.iter().find(|p| p.name == name))
            .cloned()
    }

    pub fn find_theme(&self, id: &str) -> Option<MarketplaceTheme> {
        self.community.themes.iter().find(|t| t.id == id).cloned()
    }

    // ── Reads ────────────────────────────────────────────────────────────────

    /// Synchronous slice rendered on modal open — everything currently
    /// installed on disk, irrespective of source:
    ///   * marketplace downloads (from `marketplace_installed.json`),
    ///   * dev / hand-copied plugins discovered in `plugin_dir()`,
    ///   * user themes dropped into `~/.config/arbor/themes/`.
    pub fn installed_only(&self) -> MarketplaceCatalog {
        let cat = self.catalog();
        let plugins: Vec<MarketplacePlugin> =
            cat.plugins.into_iter().filter(|p| p.installed).collect();
        let themes: Vec<MarketplaceTheme> =
            cat.themes.into_iter().filter(|t| t.installed).collect();
        MarketplaceCatalog { plugins, themes }
    }

    /// Full catalog: community + custom (from cache / fetch) + Local
    /// entries surfacing whatever lives on disk that doesn't map to a
    /// remote entry. `installed` / `enabled` are reconciled against
    /// the marketplace install ledger for remote rows, and against the
    /// manifest / theme file for Local rows (always installed=true).
    pub fn catalog(&self) -> MarketplaceCatalog {
        let installs = installs::load();

        let merge_remote = |mut p: MarketplacePlugin| -> MarketplacePlugin {
            if let Some(i) = installs.plugins.get(&p.name) {
                let catalog_version = p.version.clone();
                p.installed = true;
                p.enabled   = Some(i.enabled);
                p.installed_version = Some(i.version.clone());
                p.update_available  = newer_version(&i.version, &catalog_version);
            } else {
                p.installed = false;
                p.enabled   = None;
                p.installed_version = None;
                p.update_available  = None;
            }
            p
        };

        let mut plugins: Vec<MarketplacePlugin> =
            self.community.plugins.iter().cloned().map(&merge_remote)
                .chain(self.custom.iter().cloned().map(&merge_remote))
                .collect();

        // Merge Local plugin entries. Rules:
        //   * Skip when the name is already tracked in
        //     `marketplace_installed.json` — the remote merge above
        //     already painted that row with installed=true and the
        //     correct enable state.
        //   * Otherwise, if the name collides with a remote catalog
        //     entry, REPLACE the remote entry with the Local one (dev
        //     wins). The list MUST remain unique-by-name — the FE
        //     keyed `{#each (p.name)}` crashes otherwise.
        //   * Else append as a fresh Local row.
        let installed_via_marketplace: std::collections::HashSet<String> =
            installs.plugins.keys().cloned().collect();
        let manifests = crate::plugin::runtime::manifest::discover_plugins().unwrap_or_default();
        let states    = crate::plugin::runtime::manifest::load_plugin_states();
        for m in manifests {
            if installed_via_marketplace.contains(&m.name) { continue; }
            let enabled = states.get(&m.name).copied().unwrap_or(true);
            let local   = local_plugin_entry(m, enabled);
            match plugins.iter().position(|p| p.name == local.name) {
                Some(idx) => plugins[idx] = local,
                None      => plugins.push(local),
            }
        }
        plugins.sort_by(|a, b| a.name.cmp(&b.name));

        // Themes — community + Local (anything in user themes dir not
        // already in the community catalog).
        let mut themes: Vec<MarketplaceTheme> = self.community.themes.iter().cloned()
            .map(|mut t| {
                t.installed = installs.themes.contains_key(&t.id);
                t
            })
            .collect();
        // Same rule as plugins, including unique-by-id dedup. A
        // locally-edited theme that happens to share an id with a
        // community preset REPLACES the community row (dev wins) — and
        // the list stays unique so the FE keyed `{#each (t.id)}` never
        // sees duplicates.
        let installed_via_marketplace_themes: std::collections::HashSet<String> =
            installs.themes.keys().cloned().collect();
        for t in load_local_themes() {
            if installed_via_marketplace_themes.contains(&t.id) { continue; }
            match themes.iter().position(|x| x.id == t.id) {
                Some(idx) => themes[idx] = t,
                None      => themes.push(t),
            }
        }
        themes.sort_by(|a, b| a.name.cmp(&b.name));

        MarketplaceCatalog { plugins, themes }
    }

    // ── Custom source ────────────────────────────────────────────────────────

    /// Merge a freshly-resolved batch of plugins from a single source into
    /// the in-memory custom list. De-duplicates by name — the resolver's
    /// version wins on collision.
    pub fn merge_custom_plugins(&mut self, batch: Vec<MarketplacePlugin>) {
        for p in batch {
            if let Some(pos) = self.custom.iter().position(|x| x.name == p.name) {
                self.custom[pos] = p;
            } else {
                self.custom.push(p);
            }
        }
        cache::save_custom(&self.custom);
    }

    /// Drop every custom plugin whose entry points at `(repo, subpath)`.
    /// Used when the user removes a custom source — installed plugins
    /// keep living (the install ledger is the source-of-truth for that).
    pub fn drop_custom_by_pointer(&mut self, repo: &str, subpath: Option<&str>) {
        self.custom.retain(|p| {
            !(p.entry.repo == repo && p.entry.subpath.as_deref() == subpath)
        });
        cache::save_custom(&self.custom);
    }
}

impl Default for MarketplaceRegistry {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compare installed vs catalog version. Returns `Some(catalog)` when the
/// catalog has a strictly-newer semver; falls back to a string inequality
/// check when either side isn't valid semver (custom sources often use
/// non-semver tags). Returns `None` when the catalog isn't newer.
fn newer_version(installed: &str, catalog: &str) -> Option<String> {
    if installed == catalog || catalog == "?" { return None; }
    match (semver::Version::parse(installed), semver::Version::parse(catalog)) {
        (Ok(i), Ok(c)) if c > i => Some(catalog.to_string()),
        (Ok(_), Ok(_))          => None,
        // Non-semver — fall back to "anything different from installed
        // counts as an update". Custom-source authors often use date tags
        // (`2026-05-21`) or commit SHAs; we don't want to silently swallow
        // those.
        _ => Some(catalog.to_string()),
    }
}

/// Build a `MarketplacePlugin` from a locally-discovered manifest. Used to
/// surface dev / hand-copied plugins (or marketplace plugins after install)
/// when no remote catalog entry matches the name.
fn local_plugin_entry(m: crate::plugin::runtime::PluginManifest, enabled: bool) -> MarketplacePlugin {
    MarketplacePlugin {
        name:        m.name,
        version:     m.version.clone(),
        description: m.description,
        author:      m.author,
        category:    m.category,
        tags:        if m.keywords.is_empty() { None } else { Some(m.keywords) },
        repository:  m.repository.clone(),
        homepage:    m.homepage,
        min_arbor_version: m.min_arbor_version,
        icon:        None,
        screenshots: None,
        permissions: Some(m.permissions),
        source:      MarketplaceSource::Local,
        installed:   true,
        enabled:     Some(enabled),
        entry: RegistryEntry {
            repo:       m.repository.unwrap_or_default(),
            r#ref:      None,
            subpath:    None,
            source:     MarketplaceSource::Local,
            pinned_sha: None,
        },
        experimental:      if m.experimental { Some(true) } else { None },
        doc:               None,
        update_available:  None,
        installed_version: Some(m.version),
    }
}

/// Read every theme JSON file from `~/.config/arbor/themes/` and project
/// it onto the marketplace shape so Local themes can appear in the
/// modal alongside the community ones.
fn load_local_themes() -> Vec<MarketplaceTheme> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct RawTheme {
        id:    String,
        name:  String,
        #[serde(default)] description: Option<String>,
        #[serde(default)] author:      Option<String>,
        #[serde(default)] variant:     Option<types::ThemeVariant>,
        #[serde(default)] tags:        Option<Vec<String>>,
        #[serde(default)] vars:        std::collections::HashMap<String, String>,
    }

    let dir = crate::commands::theme_commands::themes_dir();
    let Ok(read) = std::fs::read_dir(&dir) else { return Vec::new(); };

    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        let Ok(body) = std::fs::read_to_string(&path) else { continue; };
        let Ok(raw) = serde_json::from_str::<RawTheme>(&body) else { continue; };

        let pick = |k: &str| raw.vars.get(k).cloned().unwrap_or_else(|| "#000000".into());
        let preview = types::MarketplaceThemePreview {
            bg:      pick("--bg-base"),
            fg:      pick("--text-primary"),
            accent:  pick("--accent"),
            success: pick("--success"),
            warning: pick("--warning"),
            error:   pick("--error"),
        };

        out.push(MarketplaceTheme {
            id:          raw.id,
            name:        raw.name,
            description: raw.description.unwrap_or_default(),
            author:      raw.author,
            tags:        raw.tags,
            preview,
            variant:     raw.variant,
            source:      MarketplaceSource::Local,
            installed:   true,
            entry: RegistryEntry {
                repo:       String::new(),
                r#ref:      None,
                subpath:    Some(path.to_string_lossy().to_string()),
                source:     MarketplaceSource::Local,
                pinned_sha: None,
            },
        });
    }
    out
}

/// Minimal MarketplacePlugin built from an `InstalledPlugin` — used as a
/// fallback when the community/custom catalog hasn't been fetched yet (cold
/// offline boot) so the modal still shows *something* for things we've
/// already installed.
#[allow(dead_code)]
fn installed_plugin_to_marketplace(i: &installs::InstalledPlugin) -> MarketplacePlugin {
    MarketplacePlugin {
        name:        i.name.clone(),
        version:     i.version.clone(),
        description: "(metadata unavailable offline)".into(),
        author:      "?".into(),
        category:    None,
        tags:        None,
        repository:  Some(i.entry.repo.clone()),
        homepage:    None,
        min_arbor_version: None,
        icon:        None,
        screenshots: None,
        permissions: None,
        source:      i.entry.source,
        installed:   true,
        enabled:     Some(i.enabled),
        entry:       i.entry.clone(),
        experimental: None,
        doc:         None,
        update_available:  None,
        installed_version: Some(i.version.clone()),
    }
}

/// Async helper used by the command layer: refresh the community catalog
/// from the network and stash it (also writes through to the disk cache).
/// In Phase 4 this also re-resolves every user-added custom source so a
/// single Refresh keeps both lists in sync.
pub async fn refresh_community(reg_mutex: &std::sync::Mutex<MarketplaceRegistry>) -> Result<()> {
    let repo_url = {
        let reg = reg_mutex.lock().map_err(|_| crate::error::AppError::MutexPoisoned("marketplace".into()))?;
        reg.community_repo().to_string()
    };
    let http = fetcher::client()?;
    let catalog = fetcher::fetch_catalog(&http, &repo_url, MarketplaceSource::Community).await?;
    {
        let mut reg = reg_mutex.lock().map_err(|_| crate::error::AppError::MutexPoisoned("marketplace".into()))?;
        reg.set_community(catalog);
    }
    // Best-effort custom refresh — failures here are logged per source but
    // don't fail the community refresh as a whole.
    if let Err(e) = refresh_custom(reg_mutex, &http).await {
        tracing::warn!("custom-source refresh failed: {e}");
    }
    Ok(())
}

/// Re-resolve every source in `user_registry.toml` and replace the
/// in-memory custom list. Per-source failures are dropped (logged) so a
/// single broken pointer doesn't blank the rest.
pub async fn refresh_custom(
    reg_mutex: &std::sync::Mutex<MarketplaceRegistry>,
    http:      &reqwest::Client,
) -> Result<()> {
    let sources = user_registry::load().sources;
    let mut resolved: Vec<MarketplacePlugin> = Vec::new();
    for src in sources {
        match fetcher::resolve_custom_source(http, &src.repo, src.r#ref.as_deref(), src.subpath.as_deref()).await {
            Ok(fetcher::CustomSourceResolution::Single(p)) => resolved.push(p),
            Ok(fetcher::CustomSourceResolution::Multi(plugins)) => resolved.extend(plugins),
            Err(e) => tracing::warn!(
                "custom source {} (subpath={:?}) failed to resolve: {e}",
                src.repo, src.subpath
            ),
        }
    }
    let mut reg = reg_mutex.lock().map_err(|_| crate::error::AppError::MutexPoisoned("marketplace".into()))?;
    reg.set_custom(resolved);
    Ok(())
}

/// Resolve + persist a brand-new custom source. Returns the plugins it
/// resolved to so the FE can paint them immediately.
pub async fn add_custom_source(
    reg_mutex: &std::sync::Mutex<MarketplaceRegistry>,
    source:    UserSource,
) -> Result<Vec<MarketplacePlugin>> {
    let http = fetcher::client()?;
    let res  = fetcher::resolve_custom_source(
        &http,
        &source.repo,
        source.r#ref.as_deref(),
        source.subpath.as_deref(),
    ).await?;
    let plugins: Vec<MarketplacePlugin> = match res {
        fetcher::CustomSourceResolution::Single(p)  => vec![p],
        fetcher::CustomSourceResolution::Multi(v)   => v,
    };
    // Persist the pointer first — if the resolver re-runs on Refresh
    // we'll re-fetch from the network rather than relying on the cache.
    user_registry::add(source);
    let mut reg = reg_mutex.lock().map_err(|_| crate::error::AppError::MutexPoisoned("marketplace".into()))?;
    reg.merge_custom_plugins(plugins.clone());
    Ok(plugins)
}

/// Remove a custom source. The composite key is `(repo, subpath)` so the
/// same repo can host multiple distinct sources.
pub fn remove_custom_source(
    reg_mutex: &std::sync::Mutex<MarketplaceRegistry>,
    repo:      &str,
    subpath:   Option<&str>,
) -> Result<bool> {
    let removed = user_registry::remove(repo, subpath);
    if removed {
        let mut reg = reg_mutex.lock().map_err(|_| crate::error::AppError::MutexPoisoned("marketplace".into()))?;
        reg.drop_custom_by_pointer(repo, subpath);
    }
    Ok(removed)
}
