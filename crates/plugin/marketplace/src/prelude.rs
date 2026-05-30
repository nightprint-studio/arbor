//! Canonical entry point for `arbor-plugin-marketplace`'s public API.
//!
//! Workspace convention: every Arbor library crate exposes its public
//! surface through a `prelude` module. Consumers reach types through
//! `arbor_plugin_marketplace::prelude::...` (or
//! `use arbor_plugin_marketplace::prelude::*;` once per file) rather than
//! the per-feature submodule paths. The submodules stay `pub` for rustdoc
//! navigation only.

// ── DTOs (wire shape shared with the FE) ────────────────────────────────────
pub use crate::types::{
    MarketplaceCatalog, MarketplacePlugin, MarketplaceSource, MarketplaceTheme,
    MarketplaceThemePreview, RegistryEntry, ThemeVariant,
};

// ── Errors ──────────────────────────────────────────────────────────────────
pub use crate::error::{MarketplaceError, Result as MarketplaceResult};

// ── Host capabilities ───────────────────────────────────────────────────────
pub use crate::host::MarketplaceHost;

// ── Filesystem paths (centralised; consumers should never re-derive these) ──
pub use crate::paths::{
    community_cache_file, custom_cache_file, installs_file, plugins_dir,
    themes_dir, user_registry_file,
};

// ── In-memory registry + async helpers ──────────────────────────────────────
pub use crate::registry::MarketplaceRegistry;
pub use crate::refresh::{
    add_custom_source, refresh_community, refresh_custom, remove_custom_source,
};

// ── Cache + install ledger ──────────────────────────────────────────────────
pub use crate::cache::{
    invalidate as invalidate_cache, invalidate_custom as invalidate_custom_cache,
    load_any as load_cache, load_custom as load_custom_cache, load_if_fresh,
    save as save_cache, save_custom as save_custom_cache, CacheFile, TTL_SECS,
};
pub use crate::installs::{
    forget_plugin, forget_theme, load as load_installs, record_plugin, record_theme,
    save as save_installs, set_plugin_enabled, InstalledFile, InstalledPlugin,
    InstalledTheme,
};

// ── Installer + custom-source resolver ──────────────────────────────────────
pub use crate::custom::{resolve_custom_source, CustomSourceResolution};
pub use crate::installer::{
    install_plugin, install_theme, uninstall_plugin, uninstall_theme,
};

// ── Index fetch (callers usually go through `refresh_community` instead) ────
pub use crate::index::{fetch_catalog, REGISTRY_REF, REGISTRY_REPO};

// ── User-added source pointers ──────────────────────────────────────────────
pub use crate::user_registry::{
    add as add_user_source, load as load_user_registry, remove as remove_user_source,
    save as save_user_registry, UserRegistry, UserSource,
};

// ── GitHub-API surface (URL helpers shared with consumers / docs) ───────────
pub use crate::github_api::{
    archive_url, client as github_client, github_url, join_subpath, normalise_github_url,
    parse_github_repo, raw_url, resolve_ref_sha, verify_pinned_sha, RAW_HOST,
    REQUEST_TIMEOUT,
};
