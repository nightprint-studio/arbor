// ── Marketplace IPC layer ────────────────────────────────────────────────────
//
// Typed Tauri-command wrappers used by `MarketplaceModal.svelte`. The wire
// shapes are defined in `src/lib/types/marketplace.ts`; the Rust side mirrors
// them in `src-tauri/src/marketplace/types.rs`.

import { invoke } from '@tauri-apps/api/core';
import type {
  MarketplacePlugin,
  MarketplaceTheme,
} from '$lib/types/marketplace';

/**
 * Combined catalog payload returned by the registry endpoints. The frontend
 * usually decomposes this into the `plugins` and `themes` arrays right away.
 */
export interface MarketplaceCatalog {
  plugins: MarketplacePlugin[];
  themes:  MarketplaceTheme[];
}

/**
 * Snapshot the host can return synchronously: only the entries it knows are
 * already installed (Phase 1 still reads this from the seeded mock, Phase 2+
 * will source it from the live plugin host + themes dir). Used on modal open
 * so the user sees something instantly even when the network is unreachable.
 */
export function listInstalled(): Promise<MarketplaceCatalog> {
  return invoke<MarketplaceCatalog>('marketplace_list_installed');
}

/**
 * Full catalog (installed + community + user-added custom sources). Uses the
 * host's 1h disk cache when available; transparently refreshes from the
 * network when stale. Pair with `refreshRegistry()` to force a fetch.
 */
export function fetchRegistry(): Promise<MarketplaceCatalog> {
  return invoke<MarketplaceCatalog>('marketplace_fetch_registry');
}

/**
 * Force a network fetch, bypassing the disk cache. Wired to the modal's
 * Refresh button — use when the user wants to pull new submissions before
 * the TTL ticks over.
 */
export function refreshRegistry(): Promise<MarketplaceCatalog> {
  return invoke<MarketplaceCatalog>('marketplace_refresh_registry');
}

/**
 * Names of plugins installed via the marketplace (tracked in
 * `marketplace_installed.json`). Used by the Plugin Manager to decorate
 * matching rows with a "Marketplace" badge so dev plugins are visually
 * distinguishable.
 */
export function listMarketplaceInstalledNames(): Promise<string[]> {
  return invoke<string[]>('marketplace_installed_plugin_names');
}

/**
 * Read the configured auto-refresh interval in hours. `null` means the
 * background scheduler is disabled — the user has to use the Refresh
 * button manually.
 */
export function getMarketplaceRefreshHours(): Promise<number | null> {
  return invoke<number | null>('marketplace_get_refresh_hours');
}

/**
 * Update the auto-refresh interval. Passing `null` (or `0`) disables the
 * scheduler. The change takes effect on the next poll (within 60s) — no
 * restart needed.
 */
export function setMarketplaceRefreshHours(hours: number | null): Promise<void> {
  return invoke<void>('marketplace_set_refresh_hours', { hours });
}

// ── Plugin mutations ────────────────────────────────────────────────────────

export function installPlugin(name: string): Promise<MarketplacePlugin> {
  return invoke<MarketplacePlugin>('marketplace_install_plugin', { name });
}

export function uninstallPlugin(name: string): Promise<MarketplacePlugin> {
  return invoke<MarketplacePlugin>('marketplace_uninstall_plugin', { name });
}

export function setPluginEnabled(name: string, enabled: boolean): Promise<MarketplacePlugin> {
  return invoke<MarketplacePlugin>('marketplace_set_plugin_enabled', { name, enabled });
}

// ── Theme mutations ─────────────────────────────────────────────────────────

export function installTheme(id: string): Promise<MarketplaceTheme> {
  return invoke<MarketplaceTheme>('marketplace_install_theme', { id });
}

export function uninstallTheme(id: string): Promise<MarketplaceTheme> {
  return invoke<MarketplaceTheme>('marketplace_uninstall_theme', { id });
}

// ── Custom source ───────────────────────────────────────────────────────────

export interface AddCustomSourceArgs {
  repo:        string;
  ref?:        string;
  subpath?:    string;
  pinned_sha?: string;
  description?: string;
}

/**
 * Resolve a user-supplied GitHub URL into one or more `MarketplacePlugin`
 * entries via the 3-mode resolver (subpath → root plugin.toml → root
 * index.json). The pointer is persisted to `user_registry.toml` on
 * success and survives restart.
 *
 * Returns the resolved plugins — root / subpath modes return a single
 * entry, index mode can return many.
 */
export function addCustomSource(args: AddCustomSourceArgs): Promise<MarketplacePlugin[]> {
  return invoke<MarketplacePlugin[]>('marketplace_add_custom_source', { args });
}

export interface RemoveCustomSourceArgs {
  repo:    string;
  subpath?: string;
}

/**
 * Forget a previously-added custom source. Installed plugins from this
 * source stay installed — the install ledger is independent.
 */
export function removeCustomSource(args: RemoveCustomSourceArgs): Promise<boolean> {
  return invoke<boolean>('marketplace_remove_custom_source', { args });
}
