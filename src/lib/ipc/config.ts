import { invoke } from '@tauri-apps/api/core';
import type { ActivityBarConfig, AppearanceConfig, CacheConfig, DiffConfig, GraphConfig, MrConfig, PipelinesConfig } from '$lib/types/config';
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
  invoke<RepoConfig>('get_repo_config', { tabId });

export const setRepoConfig = (tabId: string, config: RepoConfig) =>
  invoke<void>('set_repo_config', { tabId, config });

// ── Graph config ──────────────────────────────────────────────────────────────

export const getGraphConfig = () =>
  invoke<GraphConfig>('get_graph_config');

export const setGraphConfig = (config: GraphConfig) =>
  invoke<void>('set_graph_config', { config });

// ── Issues display config ────────────────────────────────────────────────────

export interface IssuesConfig {
  sort_field: string;
  sort_dir: string;
}

export const getIssuesConfig = () =>
  invoke<IssuesConfig>('get_issues_config');

export const setIssuesConfig = (config: IssuesConfig) =>
  invoke<void>('set_issues_config', { config });

// ── MR/PR Activity timeline defaults ─────────────────────────────────────────

export const getMrConfig = () =>
  invoke<MrConfig>('get_mr_config');

export const setMrConfig = (config: MrConfig) =>
  invoke<void>('set_mr_config', { config });

// ── Appearance preferences (window control style, …) ─────────────────────────

export const getAppearanceConfig = () =>
  invoke<AppearanceConfig>('get_appearance_config');

export const setAppearanceConfig = (config: AppearanceConfig) =>
  invoke<void>('set_appearance_config', { config });

// ── Recent repos (persisted in config.toml via backend) ──────────────────────

export const getRecentRepos = () =>
  invoke<string[]>('get_recent_repos');

export const addRecentRepo = (path: string) =>
  invoke<void>('add_recent_repo', { path });

// ── Cache config ──────────────────────────────────────────────────────────────

export const getCacheConfig = () =>
  invoke<CacheConfig>('get_cache_config');

export const setCacheConfig = (config: CacheConfig) =>
  invoke<void>('set_cache_config', { config });

/** Evict all backend cache entries (stats, ticket links) for a specific tab. */
export const evictTabCache = (tabId: string) =>
  invoke<void>('evict_tab_cache', { tabId });

// ── Pipelines orchestrator config (global concurrency cap) ────────────────────

export const getPipelinesConfig = () =>
  invoke<PipelinesConfig>('get_pipelines_config');

export const setPipelinesConfig = (config: PipelinesConfig) =>
  invoke<void>('set_pipelines_config', { config });

// ── Activity bar config ────────────────────────────────────────────────────────

export const getActivityBarConfig = () =>
  invoke<ActivityBarConfig>('get_activity_bar_config');

export const setActivityBarConfig = (config: ActivityBarConfig) =>
  invoke<void>('set_activity_bar_config', { config });

// ── Diff config (algorithm, context, full-file, virtualization) ──────────────

export const getDiffConfig = () =>
  invoke<DiffConfig>('get_diff_config');

export const setDiffConfig = (config: DiffConfig) =>
  invoke<void>('set_diff_config', { config });
