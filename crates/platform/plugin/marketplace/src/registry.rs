//! In-memory marketplace registry.
//!
//! Holds the community catalog and the user-resolved custom plugins (both
//! mirrored to disk in [`crate::cache`]), plus exposes the [`catalog`]
//! method that merges them with whatever the host reports as locally
//! installed (dev plugins, hand-copied folders, theme JSONs) so the
//! `MarketplaceModal` sees one unified list.
//!
//! [`catalog`]: MarketplaceRegistry::catalog

use std::collections::HashSet;
use std::sync::Arc;

use crate::cache;
use crate::host::MarketplaceHost;
use crate::index::REGISTRY_REPO;
use crate::installs;
use crate::paths;
use crate::types::{
    MarketplaceCatalog, MarketplacePlugin, MarketplaceSource, MarketplaceTheme,
    MarketplaceThemePreview, RegistryEntry, ThemeVariant,
};

pub struct MarketplaceRegistry {
    /// Community catalog from the last successful fetch (or cache restore).
    community: MarketplaceCatalog,
    /// Repo URL we fetch the community catalog from. Override-able later
    /// (settings panel / staging / fork testing).
    community_repo: String,
    /// User-added custom plugin pointers (resolved metadata).
    custom: Vec<MarketplacePlugin>,
    /// Host capabilities for the local-plugin / theme-dir merge step.
    host: Arc<dyn MarketplaceHost>,
}

impl MarketplaceRegistry {
    pub fn new(host: Arc<dyn MarketplaceHost>) -> Self {
        let community = cache::load_any()
            .map(|f| f.catalog)
            .unwrap_or_default();
        // Cold-start fallback: prefer the last cached resolution of custom
        // sources so an offline boot still paints something. Refresh
        // replaces this whenever the user opens the modal.
        let custom = cache::load_custom();
        Self {
            community,
            community_repo: REGISTRY_REPO.to_string(),
            custom,
            host,
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
    ///   * dev / hand-copied plugins discovered in the host plugin dir,
    ///   * user themes dropped into the themes dir.
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
        let installed_via_marketplace: HashSet<String> =
            installs.plugins.keys().cloned().collect();
        let manifests = self.host.discover_plugins();
        let states    = self.host.plugin_states();
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
        let installed_via_marketplace_themes: HashSet<String> =
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
fn local_plugin_entry(
    m:       arbor_plugin_types::prelude::Manifest,
    enabled: bool,
) -> MarketplacePlugin {
    let dependencies = m.dependencies.clone();
    let repository_for_entry = m.repository.clone().unwrap_or_default();
    MarketplacePlugin {
        name:        m.name,
        version:     m.version.clone(),
        description: m.description,
        author:      m.author,
        category:    m.category,
        tags:        if m.keywords.is_empty() { None } else { Some(m.keywords) },
        repository:  m.repository,
        homepage:    m.homepage,
        min_arbor_version: m.min_arbor_version,
        icon:        None,
        screenshots: None,
        permissions: Some(m.permissions),
        source:      MarketplaceSource::Local,
        installed:   true,
        enabled:     Some(enabled),
        entry: RegistryEntry {
            repo:       repository_for_entry,
            r#ref:      None,
            subpath:    None,
            source:     MarketplaceSource::Local,
            pinned_sha: None,
            external:   false,
        },
        experimental:      if m.experimental { Some(true) } else { None },
        doc:               None,
        update_available:  None,
        installed_version: Some(m.version),
        dependencies,
    }
}

/// Read every theme JSON file from the host's themes dir and project it
/// onto the marketplace shape so Local themes can appear in the modal
/// alongside the community ones.
fn load_local_themes() -> Vec<MarketplaceTheme> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct RawTheme {
        id:    String,
        name:  String,
        #[serde(default)] description: Option<String>,
        #[serde(default)] author:      Option<String>,
        #[serde(default)] variant:     Option<ThemeVariant>,
        #[serde(default)] tags:        Option<Vec<String>>,
        #[serde(default)] vars:        std::collections::HashMap<String, String>,
    }

    let dir = paths::themes_dir();
    let Ok(read) = std::fs::read_dir(&dir) else { return Vec::new(); };

    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        let Ok(body) = std::fs::read_to_string(&path) else { continue; };
        let Ok(raw) = serde_json::from_str::<RawTheme>(&body) else { continue; };

        let pick = |k: &str| raw.vars.get(k).cloned().unwrap_or_else(|| "#000000".into());
        let preview = MarketplaceThemePreview {
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
                external:   false,
            },
        });
    }
    out
}
