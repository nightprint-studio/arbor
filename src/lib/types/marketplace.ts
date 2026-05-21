// ── Marketplace types ─────────────────────────────────────────────────────────
//
// Shapes used by `MarketplaceModal.svelte` and (later) by the resolver +
// installer that fetch `plugin.toml` / theme JSON from `arbor-registry`.
//
// The registry file (`index.json` in the `arbor-registry` repo) is a *pointer
// list*: each entry says "go here, optionally at this ref/subpath". The client
// then fetches the source-of-truth metadata (`plugin.toml` or theme `.json`)
// from the raw URL and shows it in the UI.  No duplicated metadata in the
// registry → authors update one file (their own plugin.toml).

import type { PluginPermissions } from './plugin';

// ─── Tab + source classification ──────────────────────────────────────────────

export type MarketplaceTab = 'plugins' | 'themes';

/**
 * Where a listing came from.
 *  - `community` — listed in the `arbor-extensions` repo (vetted via PR review).
 *  - `custom`    — the user added the git URL by hand.
 *  - `local`     — installed on disk with no matching marketplace entry (zip
 *                  sideload, dev folder, …). Surfaces only for plugins that
 *                  the host already has but the registry doesn't know about.
 */
export type MarketplaceSource = 'community' | 'custom' | 'local';

// ─── Registry pointer ────────────────────────────────────────────────────────

/**
 * A single entry as it lives in the registry's `index.json` (or in the user's
 * `user_registry.toml` when source = 'custom'). The resolver uses this to
 * locate the plugin/theme content; the *metadata* (name, description, …) is
 * always read from the source-of-truth file inside the repo.
 */
export interface RegistryEntry {
  /** GitHub repository URL — `https://github.com/<owner>/<repo>`. */
  repo: string;
  /**
   * Git ref (tag, branch, or commit SHA).
   *  - Curated entries (source = 'community') are expected to point at the
   *    *latest tag*; the resolver picks it automatically when this field is
   *    empty, falling back to `main` if the repo has no tags yet.
   *  - For custom entries the user can supply an explicit `ref`.
   */
  ref?: string;
  /**
   * Subpath inside the repo when the project hosts multiple plugins
   * (e.g. `plugins/foo`). When empty the plugin/theme lives at the root.
   */
  subpath?: string;
  /** Where this pointer came from. */
  source: MarketplaceSource;
  /**
   * Optional commit SHA pin — if set, the installer verifies the resolved
   * ref points at this exact commit and refuses otherwise. Useful for
   * `custom` sources to defend against tag-hijack.
   */
  pinned_sha?: string;
}

// ─── Resolved plugin metadata ─────────────────────────────────────────────────

/**
 * Plugin entry as shown in the marketplace list / detail pane. Built by the
 * resolver after fetching `plugin.toml` for a `RegistryEntry`.
 */
export interface MarketplacePlugin {
  /** Stable identifier — `name` from `plugin.toml`. */
  name:        string;
  version:     string;
  description: string;
  author:      string;
  /**
   * Free-form category — used for filtering. Curated values:
   *   `build` · `ci` · `git-workflow` · `language` · `ui` · `data` · `theme`
   * Plugins without a category appear under "Other".
   */
  category?:   string;
  tags?:       string[];
  /** Canonical repository URL (often matches `entry.repo`). */
  repository?: string;
  /** External docs/homepage link. */
  homepage?:   string;
  /** Minimum Arbor app version (semver) declared in `plugin.toml`. */
  min_arbor_version?: string;
  /**
   * Resolved icon URL (raw.githubusercontent.com/…). When absent the modal
   * falls back to a monogram derived from `name`.
   */
  icon?:       string;
  /** Resolved screenshot URLs. */
  screenshots?: string[];
  /** Mirrored from `[permissions]` so the install confirmation can render them. */
  permissions?: PluginPermissions;
  /** Where this listing came from. */
  source:      MarketplaceSource;
  /** True if a folder with this name already lives in `plugins/`. */
  installed:   boolean;
  /**
   * When the plugin is installed, mirrors the host's enable state so the
   * detail pane can render an in-place toggle without re-opening the
   * Plugin Manager. Undefined when `installed === false`.
   */
  enabled?:    boolean;
  /** The pointer that produced this entry. */
  entry:       RegistryEntry;
  /** When true, the entry is flagged experimental in its manifest. */
  experimental?: boolean;
  /**
   * HTML documentation string — sourced from the plugin's `doc_file`
   * declared in `plugin.toml`. The fetcher reads it from the repo when
   * available so the marketplace can render the same docs the Plugin
   * Manager already shows on installed plugins.
   */
  doc?:        string;
  /**
   * When set, the installed version is older than the catalog version
   * and the user can hit "Update" to re-run the install path. Carries
   * the *newer* version string for display ("v1.2 → v1.3"). Undefined
   * when no update is available (or the plugin is not installed).
   */
  update_available?:  string;
  /** The version currently on disk per `marketplace_installed.json`. */
  installed_version?: string;
}

// ─── Resolved theme metadata ──────────────────────────────────────────────────

export interface MarketplaceThemePreview {
  bg:      string;
  fg:      string;
  accent:  string;
  success: string;
  warning: string;
  error:   string;
}

/**
 * Theme entry — themes ship as a single JSON file (`themes/<id>.json`) with
 * shape `{ id, name, description, built_in, vars }`. The marketplace
 * extracts a 6-swatch preview from `vars` to render colour chips in the card.
 */
export interface MarketplaceTheme {
  /** `id` from the theme JSON — also doubles as install key. */
  id:           string;
  name:         string;
  description:  string;
  author?:      string;
  tags?:        string[];
  /** Six representative CSS-var colours used for the card preview. */
  preview:      MarketplaceThemePreview;
  /** Quick visual variant — derived from background luminance when missing. */
  variant?:     'dark' | 'light';
  source:       MarketplaceSource;
  installed:    boolean;
  entry:        RegistryEntry;
}

// ─── Filter state (drives the toolbar) ────────────────────────────────────────

/** Two-state filter (selected list + "all" sentinel via empty array). */
export interface MarketplaceFilter {
  /** Free-text query (matched against name + description + tags + author). */
  search:        string;
  /** Selected categories — empty array means "all". */
  categories:    string[];
  /** Selected tags — empty array means "all". */
  tags:          string[];
  /** Selected sources — empty array means "all". */
  sources:       MarketplaceSource[];
  /** Visibility filter for the two main sections. */
  installation:  'all' | 'installed' | 'available';
  /** Light/dark restriction — only meaningful on the Themes tab. */
  themeVariant:  'all' | 'dark' | 'light';
}

export function emptyFilter(): MarketplaceFilter {
  return {
    search:       '',
    categories:   [],
    tags:         [],
    sources:      [],
    installation: 'all',
    themeVariant: 'all',
  };
}
