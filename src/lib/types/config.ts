export type DiffMode = 'unified' | 'split' | 'word_diff';
export type DiffAlgorithm = 'myers' | 'patience' | 'histogram';
export type FileListView = 'list' | 'tree';

export interface ThemeConfig {
  name: string;
}

export interface DiffConfig {
  algorithm: DiffAlgorithm;
  context_lines: number;
  word_wrap: boolean;
  /** Render the entire file as context. */
  full_file: boolean;
  /** Switch to virtualized rendering above this line count. */
  virt_threshold: number;
  /** Layout used by DiffViewer: split (side-by-side) vs unified. */
  mode: DiffMode;
  /** Layout used by FileDiffList: flat list vs folder tree. */
  file_list_view: FileListView;
  /** Show a confirmation dialog before discarding workdir changes. */
  confirm_discard: boolean;
}

export interface GraphConfig {
  page_size: number;
  show_remote_branches: boolean;
  show_tags: boolean;
  /** When false the entire history is loaded at once (no lazy-load on scroll). */
  paginate: boolean;
  /** Render the ticket-link chip column in the commit graph. */
  ticket_links_enabled: boolean;
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
export type AnimSpeed = 'fast' | 'normal' | 'slow';

/** Visual tweaks that don't belong to theme or layout: window-control button
 *  style, global font scale, and the opt-in for per-theme font preferences. */
export interface AppearanceConfig {
  window_controls_style: WindowControlsStyle;
  /** Global UI font scale multiplier applied to `--font-scale`. */
  font_scale: number;
  /** When true the active theme's `--theme-font-*` win over the global font stack. */
  use_theme_fonts: boolean;
}

/** UI animation preferences. `enabled=false` collapses every transition
 *  duration to 0ms; `speed` scales the base durations otherwise. */
export interface AnimationsConfig {
  enabled: boolean;
  speed: AnimSpeed;
}

/** Host-wide commit preferences. Per-repo overrides live in
 *  `.arbor/config.toml`; this is the global fallback. */
export interface CommitConfig {
  /** Fallback commit-message template used when the repo has no native
   *  `commit.template` configured. Empty string disables the template. */
  template_global: string;
}

/** First-run onboarding tour state. Persisted so the welcome wizard
 *  only auto-pops the very first time. `version` is a schema knob: when
 *  we add meaningful new steps the frontend bumps the current version
 *  and the modal re-opens for users on an older one. */
export interface OnboardingConfig {
  completed: boolean;
  version:   number;
}

export interface AppConfig {
  theme: ThemeConfig;
  diff: DiffConfig;
  graph: GraphConfig;
  recent_repos: string[];
  cache: CacheConfig;
  activity_bar: ActivityBarConfig;
  mr: MrConfig;
  appearance: AppearanceConfig;
  animations: AnimationsConfig;
  commit: CommitConfig;
}
