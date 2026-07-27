import { invoke } from '@tauri-apps/api/core';
import { corvus, platform, sitta } from './rpc';
import type { ActivityBarConfig, AnimationsConfig, AppearanceConfig, BranchGroupingConfig, BranchesConfig, CacheConfig, CommitConfig, DiffConfig, ExplorerConfig, GraphConfig, MrConfig, OnboardingConfig, PipelinesConfig, SittaConfig, TytoConfig, WhatsNewConfig } from '$lib/types/config';
import type { TicketLinksRepoConfig } from '$lib/types/corvus/git';

export type { TicketLinksRepoConfig };

export interface RepoUserConfig {
  name?: string;
  email?: string;
}

export interface StatsExcludeConfig {
  extensions: string[];
  folders: string[];
  files: string[];
}

export interface RepoConfig {
  display_name?: string;
  default_remote?: string;
  pinned_branches: string[];
  user: RepoUserConfig;
  issue_tracker?: string;
  issue_tracker_project_id?: string;
  ticket_links?: TicketLinksRepoConfig;
  stats_exclude?: StatsExcludeConfig;
}

export const getRepoConfig = (tabId: string) =>
  corvus<RepoConfig>('get_repo_config', { tab_id: tabId });

export const setRepoConfig = (tabId: string, config: RepoConfig) =>
  corvus<void>('set_repo_config', { tab_id: tabId, config });

// ── Graph config ──────────────────────────────────────────────────────────────

export const getGraphConfig = () =>
  corvus<GraphConfig>('get_graph_config');

export const setGraphConfig = (config: GraphConfig) =>
  corvus<void>('set_graph_config', { config });

// ── Issues display config ────────────────────────────────────────────────────

export interface IssuesConfig {
  sort_field: string;
  sort_dir: string;
}

export const getIssuesConfig = () =>
  corvus<IssuesConfig>('get_issues_config');

export const setIssuesConfig = (config: IssuesConfig) =>
  corvus<void>('set_issues_config', { config });

// ── MR/PR Activity timeline defaults ─────────────────────────────────────────

export const getMrConfig = () =>
  corvus<MrConfig>('get_mr_config');

export const setMrConfig = (config: MrConfig) =>
  corvus<void>('set_mr_config', { config });

// ── Appearance preferences (window control style, …) ─────────────────────────

export const getAppearanceConfig = () =>
  platform<AppearanceConfig>('get_appearance_config');

export const setAppearanceConfig = (config: AppearanceConfig) =>
  platform<void>('set_appearance_config', { config });

// ── File explorer: launcher-side integration settings ───────────────────────
// The 4 settings the shell consumes (OS-global shortcut + accel, always-new-
// window, reveal-in-builtin). The explorer's own UX prefs live in the sitta
// config below.

export const getExplorerConfig = () =>
  platform<ExplorerConfig>('get_explorer_config');

// set_explorer_config stays a Tauri command (keep-shell: reconciles an
// OS-global shortcut via AppHandle), so it keeps using `invoke`.
export const setExplorerConfig = (config: ExplorerConfig) =>
  invoke<void>('set_explorer_config', { config });

// ── Tyto (screen recorder): launcher-side integration settings ───────────────
// The opt-in OS-global shortcut that opens the recorder window. Read even when a
// Tyto backend isn't running (the launcher registers the hotkey at boot).

export const getTytoConfig = () =>
  platform<TytoConfig>('get_tyto_config');

// set_tyto_config stays a Tauri command (keep-shell: reconciles an OS-global
// shortcut via AppHandle), so it uses `invoke` — mirrors setExplorerConfig.
export const setTytoConfig = (config: TytoConfig) =>
  invoke<void>('set_tyto_config', { config });

// ── File explorer: sitta's own UX preferences (out-of-process) ───────────────
// View/sort/startup, sidebar + column layout, favourites, saved searches,
// external-link policy, and the git-awareness switch — owned by `sitta-be`.

export const getSittaConfig = () =>
  sitta<SittaConfig>('get_sitta_config');

export const setSittaConfig = (config: SittaConfig) =>
  sitta<void>('set_sitta_config', { config });

// ── Launcher (Canopy) preferences — per product ──────────────────────────────

export interface ProductLauncherConfig {
  /** When true, closing this product's window keeps it running (tray-style) and
   *  it's terminated only via the launcher's Stop; when false closing it ends it. */
  close_to_tray: boolean;
}

/** Where workspace products open. Default is per-OS: `tabbed` on macOS (no
 *  per-window taskbar there), `windows` elsewhere. */
export type WindowMode = 'windows' | 'tabbed';

export interface LauncherConfig {
  /** Per-product preferences, keyed by Canopy product id. */
  products: Record<string, ProductLauncherConfig>;
  window_mode: WindowMode;
}

// Keep-shell commands (read by the native window-event handler).
export const getLauncherConfig = () =>
  invoke<LauncherConfig>('get_launcher_config');

// Direct Tauri command → args are camelCase (Tauri maps `closeToTray` to the
// Rust `close_to_tray` param). The snake_case convention only applies to the
// `platform`/rpc path, where params are serde-deserialized payload fields.
export const setLauncherCloseToTray = (id: string, closeToTray: boolean) =>
  invoke<void>('set_launcher_close_to_tray', { id, closeToTray });

/** Switch between one-window-per-product and the tabbed container. Applies to
 *  the next launch; windows already open stay where they are. */
export const setLauncherWindowMode = (mode: WindowMode) =>
  invoke<void>('set_launcher_window_mode', { mode });

// ── Recent repos (persisted in config.toml via backend) ──────────────────────

export const getRecentRepos = () =>
  platform<string[]>('get_recent_repos');

export const addRecentRepo = (path: string) =>
  platform<void>('add_recent_repo', { path });

// ── Cache config ──────────────────────────────────────────────────────────────

export const getCacheConfig = () =>
  corvus<CacheConfig>('get_cache_config');

export const setCacheConfig = (config: CacheConfig) =>
  corvus<void>('set_cache_config', { config });

/** Evict all backend cache entries (stats, ticket links) for a specific tab. */
export const evictTabCache = (tabId: string) =>
  platform<void>('evict_tab_cache', { tab_id: tabId });

// ── Pipelines orchestrator config (global concurrency cap) ────────────────────

export const getPipelinesConfig = () =>
  corvus<PipelinesConfig>('get_pipelines_config');

export const setPipelinesConfig = (config: PipelinesConfig) =>
  corvus<void>('set_pipelines_config', { config });

// ── Activity bar config ────────────────────────────────────────────────────────

export const getActivityBarConfig = () =>
  platform<ActivityBarConfig>('get_activity_bar_config');

export const setActivityBarConfig = (config: ActivityBarConfig) =>
  platform<void>('set_activity_bar_config', { config });

// ── Diff config (algorithm, context, full-file, virtualization) ──────────────

export const getDiffConfig = () =>
  corvus<DiffConfig>('get_diff_config');

export const setDiffConfig = (config: DiffConfig) =>
  corvus<void>('set_diff_config', { config });

// ── Animations config (enabled + speed multiplier) ────────────────────────────

export const getAnimationsConfig = () =>
  platform<AnimationsConfig>('get_animations_config');

export const setAnimationsConfig = (config: AnimationsConfig) =>
  platform<void>('set_animations_config', { config });

// ── Commit config (global template fallback, …) ───────────────────────────────

export const getCommitConfig = () =>
  corvus<CommitConfig>('get_commit_config');

export const setCommitConfig = (config: CommitConfig) =>
  corvus<void>('set_commit_config', { config });

// ── Onboarding tour state ─────────────────────────────────────────────────────

// Onboarding is per-product now: the git product's first-run tour is owned by
// corvus-be (corvus/config.toml). Other products route to their own backend.
export const getOnboardingConfig = () =>
  corvus<OnboardingConfig>('get_onboarding_config');

export const setOnboardingConfig = (config: OnboardingConfig) =>
  corvus<void>('set_onboarding_config', { config });

// ── What's New modal state (last-seen app version) ──────────────────────────

export const getWhatsNewConfig = () =>
  platform<WhatsNewConfig>('get_whats_new_config');

export const setWhatsNewConfig = (config: WhatsNewConfig) =>
  platform<void>('set_whats_new_config', { config });

// ── Branches sidebar (global recursive split + per-repo grouping state) ──────

export const getBranchesConfig = () =>
  corvus<BranchesConfig>('get_branches_config');

export const setBranchesConfig = (config: BranchesConfig) =>
  corvus<void>('set_branches_config', { config });

export const getBranchGrouping = (tabId: string) =>
  corvus<BranchGroupingConfig>('get_branch_grouping', { tab_id: tabId });

export const setBranchGrouping = (tabId: string, config: BranchGroupingConfig) =>
  corvus<void>('set_branch_grouping', { tab_id: tabId, config });
