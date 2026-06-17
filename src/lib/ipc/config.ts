import { invoke } from '@tauri-apps/api/core';
import { platform } from './rpc';
import type { ActivityBarConfig, AnimationsConfig, AppearanceConfig, BranchGroupingConfig, BranchesConfig, CacheConfig, CommitConfig, DiffConfig, ExplorerConfig, GraphConfig, MrConfig, OnboardingConfig, PipelinesConfig, WhatsNewConfig } from '$lib/types/config';
import type { TicketLinksRepoConfig } from '$lib/types/git';

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
  platform<RepoConfig>('get_repo_config', { tab_id: tabId });

export const setRepoConfig = (tabId: string, config: RepoConfig) =>
  platform<void>('set_repo_config', { tab_id: tabId, config });

// ── Graph config ──────────────────────────────────────────────────────────────

export const getGraphConfig = () =>
  platform<GraphConfig>('get_graph_config');

export const setGraphConfig = (config: GraphConfig) =>
  platform<void>('set_graph_config', { config });

// ── Issues display config ────────────────────────────────────────────────────

export interface IssuesConfig {
  sort_field: string;
  sort_dir: string;
}

export const getIssuesConfig = () =>
  platform<IssuesConfig>('get_issues_config');

export const setIssuesConfig = (config: IssuesConfig) =>
  platform<void>('set_issues_config', { config });

// ── MR/PR Activity timeline defaults ─────────────────────────────────────────

export const getMrConfig = () =>
  platform<MrConfig>('get_mr_config');

export const setMrConfig = (config: MrConfig) =>
  platform<void>('set_mr_config', { config });

// ── Appearance preferences (window control style, …) ─────────────────────────

export const getAppearanceConfig = () =>
  platform<AppearanceConfig>('get_appearance_config');

export const setAppearanceConfig = (config: AppearanceConfig) =>
  platform<void>('set_appearance_config', { config });

// ── File explorer preferences (git awareness, global shortcut, display) ──────

export const getExplorerConfig = () =>
  platform<ExplorerConfig>('get_explorer_config');

// set_explorer_config stays a Tauri command (keep-shell: reconciles an
// OS-global shortcut via AppHandle), so it keeps using `invoke`.
export const setExplorerConfig = (config: ExplorerConfig) =>
  invoke<void>('set_explorer_config', { config });

// ── Recent repos (persisted in config.toml via backend) ──────────────────────

export const getRecentRepos = () =>
  platform<string[]>('get_recent_repos');

export const addRecentRepo = (path: string) =>
  platform<void>('add_recent_repo', { path });

// ── Cache config ──────────────────────────────────────────────────────────────

export const getCacheConfig = () =>
  platform<CacheConfig>('get_cache_config');

export const setCacheConfig = (config: CacheConfig) =>
  platform<void>('set_cache_config', { config });

/** Evict all backend cache entries (stats, ticket links) for a specific tab. */
export const evictTabCache = (tabId: string) =>
  platform<void>('evict_tab_cache', { tab_id: tabId });

// ── Pipelines orchestrator config (global concurrency cap) ────────────────────

export const getPipelinesConfig = () =>
  platform<PipelinesConfig>('get_pipelines_config');

export const setPipelinesConfig = (config: PipelinesConfig) =>
  platform<void>('set_pipelines_config', { config });

// ── Activity bar config ────────────────────────────────────────────────────────

export const getActivityBarConfig = () =>
  platform<ActivityBarConfig>('get_activity_bar_config');

export const setActivityBarConfig = (config: ActivityBarConfig) =>
  platform<void>('set_activity_bar_config', { config });

// ── Diff config (algorithm, context, full-file, virtualization) ──────────────

export const getDiffConfig = () =>
  platform<DiffConfig>('get_diff_config');

export const setDiffConfig = (config: DiffConfig) =>
  platform<void>('set_diff_config', { config });

// ── Animations config (enabled + speed multiplier) ────────────────────────────

export const getAnimationsConfig = () =>
  platform<AnimationsConfig>('get_animations_config');

export const setAnimationsConfig = (config: AnimationsConfig) =>
  platform<void>('set_animations_config', { config });

// ── Commit config (global template fallback, …) ───────────────────────────────

export const getCommitConfig = () =>
  platform<CommitConfig>('get_commit_config');

export const setCommitConfig = (config: CommitConfig) =>
  platform<void>('set_commit_config', { config });

// ── Onboarding tour state ─────────────────────────────────────────────────────

export const getOnboardingConfig = () =>
  platform<OnboardingConfig>('get_onboarding_config');

export const setOnboardingConfig = (config: OnboardingConfig) =>
  platform<void>('set_onboarding_config', { config });

// ── What's New modal state (last-seen app version) ──────────────────────────

export const getWhatsNewConfig = () =>
  platform<WhatsNewConfig>('get_whats_new_config');

export const setWhatsNewConfig = (config: WhatsNewConfig) =>
  platform<void>('set_whats_new_config', { config });

// ── Branches sidebar (global recursive split + per-repo grouping state) ──────

export const getBranchesConfig = () =>
  platform<BranchesConfig>('get_branches_config');

export const setBranchesConfig = (config: BranchesConfig) =>
  platform<void>('set_branches_config', { config });

export const getBranchGrouping = (tabId: string) =>
  platform<BranchGroupingConfig>('get_branch_grouping', { tab_id: tabId });

export const setBranchGrouping = (tabId: string, config: BranchGroupingConfig) =>
  platform<void>('set_branch_grouping', { tab_id: tabId, config });
