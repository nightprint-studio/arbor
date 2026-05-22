export type DiffMode = 'unified' | 'split' | 'word_diff';
export type DiffAlgorithm = 'myers' | 'patience' | 'histogram';

export interface ThemeConfig {
  name: string;
}

export interface LayoutConfig {
  sidebar_width: number;
  bottom_panel_height: number;
  diff_mode: DiffMode;
}

export interface DiffConfig {
  algorithm: DiffAlgorithm;
  context_lines: number;
  word_wrap: boolean;
  /** Render the entire file as context. */
  full_file: boolean;
  /** Switch to virtualized rendering above this line count. */
  virt_threshold: number;
}

export interface GraphConfig {
  page_size: number;
  show_remote_branches: boolean;
  show_tags: boolean;
  /** When false the entire history is loaded at once (no lazy-load on scroll). */
  paginate: boolean;
}

export interface CacheConfig {
  enabled: boolean;
  max_tabs: number;
  refresh_interval_secs: number;
  scheduler_enabled: boolean;
  /** Automatically evict idle tab caches. */
  auto_evict_enabled: boolean;
  /** Seconds a tab must be inactive before its backend cache is evicted. */
  tab_idle_secs: number;
  /** How often the eviction scheduler checks for idle tabs (seconds). */
  evict_check_interval_secs: number;
  /** Also drop the git2 Repository handle on eviction to free libgit2 caches. */
  close_repo_on_evict: boolean;
  /** Minimum number of most-recently-used tabs to always keep in cache,
   *  regardless of idle time. The active tab counts toward this total. */
  min_cached_tabs: number;
  /** TTL (seconds) for the Repository Browser repo list cache.
   *  Zero disables the cache. */
  repo_browser_ttl_secs: number;
}

export interface ActivityBarItemConfig {
  id: string;
  visible: boolean;
}

export interface ActivityBarConfig {
  top_items: ActivityBarItemConfig[];
  bottom_items: ActivityBarItemConfig[];
  /** Ordering + visibility for the right-side ActivityBar (plugins only). */
  right_top_items?: ActivityBarItemConfig[];
  right_bottom_items?: ActivityBarItemConfig[];
}

/** Activity-timeline filter defaults for the MR/PR detail modal.
 *  Each flag controls the initial state of the matching filter chip;
 *  toggling a chip in the modal does NOT persist back to this config. */
export interface MrConfig {
  default_show_comments: boolean;
  default_show_bots:     boolean;
  default_show_activity: boolean;
}

/** Pipelines orchestrator settings (global concurrency cap, …).
 *  Mirrors `app_config::PipelinesConfig` on the Rust side. */
export interface PipelinesConfig {
  /** Max concurrent pipeline runs across all plugins. 0 = unlimited. */
  max_concurrent_runs: number;
}

export type WindowControlsStyle = 'mac' | 'windows';

/** Visual tweaks that don't belong to theme or layout — currently only the
 *  window-control button style. Position and dimensions of the controls are
 *  intentionally fixed regardless of style. */
export interface AppearanceConfig {
  window_controls_style: WindowControlsStyle;
}

export interface AppConfig {
  theme: ThemeConfig;
  layout: LayoutConfig;
  diff: DiffConfig;
  graph: GraphConfig;
  recent_repos: string[];
  cache: CacheConfig;
  activity_bar: ActivityBarConfig;
  mr: MrConfig;
  appearance: AppearanceConfig;
}
