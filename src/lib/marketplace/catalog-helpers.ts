// ── Catalog helpers ──────────────────────────────────────────────────────────
//
// Pure functions that derive filter/sort metadata from a `MarketplaceCatalog`.
// Kept separate from the IPC layer so the modal can reuse them on locally
// merged state (e.g. after an install) without re-hitting the backend.

import type { MarketplacePlugin, MarketplaceTheme } from '$lib/types/marketplace';

/** Distinct plugin categories, sorted alphabetically. `undefined` → `"other"`. */
export function pluginCategories(plugins: MarketplacePlugin[]): string[] {
  return Array.from(new Set(plugins.map(p => p.category ?? 'other')))
    .sort((a, b) => a.localeCompare(b));
}

/** Distinct plugin tags, sorted alphabetically. */
export function pluginTags(plugins: MarketplacePlugin[]): string[] {
  const all = new Set<string>();
  for (const p of plugins) for (const t of p.tags ?? []) all.add(t);
  return Array.from(all).sort((a, b) => a.localeCompare(b));
}

/** Distinct theme tags, sorted alphabetically. */
export function themeTags(themes: MarketplaceTheme[]): string[] {
  const all = new Set<string>();
  for (const t of themes) for (const tag of t.tags ?? []) all.add(tag);
  return Array.from(all).sort((a, b) => a.localeCompare(b));
}
