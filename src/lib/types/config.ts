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
  /** Visual tab width used when rendering diff lines containing `\t`.
   *  Clamped to [1, 16] at read; persisted as-is. */
  tab_width: number;
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

/** Stable id of a commit-graph column.
 *  `graph` is the SVG lane diagram; the rest are text cells. */
export type GraphColumnId = 'graph' | 'refs' | 'subject' | 'author' | 'date' | 'hash';

export interface GraphColumn {
  id: GraphColumnId;
  /** Track width in px. Semantics depend on the id:
   *  - `graph`   → MAX width; the track auto-sizes to `svgW + 12` and
   *                caps at this value (cap softens when the natural lane
   *                count would exceed it, so no lanes get clipped).
   *  - `subject` → MIN width; column flex-grows past it via `minmax(W, 1fr)`.
   *  - everything else → fixed track width. */
  width: number;
  visible: boolean;
}

export interface GraphColumnsConfig {
  /** Ordered column list. Index 0 is leftmost; last is rightmost. */
  columns: GraphColumn[];
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

/** Global Branches-sidebar behaviour. Per-repo on/off lives in
 *  RepoConfig.branch_grouping.enabled. */
export interface BranchesConfig {
  /** Recursive `/` split (GitKraken/Fork) when true; single-level (JetBrains) when false. */
  grouping_recursive: boolean;
}

/** Per-repo branch-grouping state — enabled flag + collapsed group paths. */
export interface BranchGroupingConfig {
  enabled: boolean;
  collapsed_groups: string[];
}

export type WindowControlsStyle = 'mac' | 'windows';
export type AnimSpeed = 'fast' | 'normal' | 'slow';
export type ActivityBarPosition = 'left' | 'right' | 'hidden';

/** Visual tweaks that don't belong to theme or layout: window-control button
 *  style, global font scale, and the opt-in for per-theme font preferences. */
export interface AppearanceConfig {
  window_controls_style: WindowControlsStyle;
  /** Global UI font scale multiplier applied to `--font-scale`. */
  font_scale: number;
  /** When true the active theme's `--theme-font-*` win over the global font stack. */
  use_theme_fonts: boolean;
  /** Position of the built-in activity bar. `hidden` collapses it and
   *  reveals it on hover of the left edge. */
  activity_bar_position: ActivityBarPosition;
  /** Reduced title-bar height + tighter padding. */
  compact_title_bar: boolean;
  /** Maximum number of dialogs that can be minimized at the same time
   *  (status-bar parked-dialogs panel). Clamped to `[1, 20]` at read time. */
  parked_modals_max: number;
  /** IntelliJ-style "compact middle packages" — collapse chains of
   *  single-child directories into one row across file panel, stage area,
   *  commit detail file list, and conflict sidebar. */
  compact_file_tree_dirs: boolean;
}

/** View mode for the built-in file explorer's listing. */
export type ExplorerView = 'details' | 'medium' | 'large' | 'xlarge';
/** Column the explorer listing sorts by. */
export type ExplorerSort = 'name' | 'modified' | 'size';
/** What a freshly-opened explorer tab shows. */
export type ExplorerStartup = 'overview' | 'last';

/** One explorer sidebar section's persisted order + visibility. */
export interface ExplorerSectionConfig {
  id: string;
  visible: boolean;
}

/** Built-in file explorer preferences. `git_awareness` + `global_shortcut`
 *  are host-level switches (also editable from the SettingsPanel); the display
 *  defaults are edited from the explorer's own in-window settings page. */
export interface ExplorerConfig {
  /** Master switch for git awareness (status overlays, repo markers, Changes
   *  panel, branch switch). Off by default — when off, no git IPC is issued. */
  git_awareness: boolean;
  /** Register the OS-global shortcut that opens the explorer window. Off by
   *  default; toggling re-registers/unregisters at runtime. */
  global_shortcut: boolean;
  /** Default view mode for not-yet-visited folders. */
  default_view: ExplorerView;
  /** Show dot-prefixed (hidden) entries by default. */
  show_hidden: boolean;
  /** Default state of recursive (subfolder) search. */
  recursive_search: boolean;
  /** Accelerator for the global shortcut (Tauri format, e.g. "Ctrl+Shift+E"). */
  global_shortcut_accel: string;
  /** Default sort column for the listing. */
  default_sort: ExplorerSort;
  /** Default sort direction (ascending when true). */
  sort_ascending: boolean;
  /** What a freshly-opened explorer tab shows. */
  startup: ExplorerStartup;
  /** When true, opening the explorer always spawns a new window instead of
   *  focusing the existing one. */
  always_new_window: boolean;
  /** Maximum number of recent folders kept in the sidebar (1–50). */
  max_recents: number;
  /** Sidebar section order + visibility. Empty → built-in order, all shown. */
  sidebar_sections: ExplorerSectionConfig[];
  /** Allow opening generic external links (custom schemes) typed in the
   *  address bar via the OS handler. Off by default; each open still prompts
   *  unless the scheme was remembered. */
  open_external_links: boolean;
  /** Additionally allow http/https links from the address bar to open in the
   *  browser. Gated behind `open_external_links`; off by default. */
  open_web_links: boolean;
  /** Schemes (lower-cased) the user chose to remember, so they open without
   *  prompting (e.g. ["vscode", "https"]). */
  remembered_external_schemes: string[];
  /** Route the app's "Open / Reveal in File Explorer" actions into the built-in
   *  explorer window instead of the OS file manager. Off by default. */
  reveal_in_builtin: boolean;
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

/** "What's New" modal state. The frontend compares the current app version
 *  against `last_seen_version` on boot and auto-opens the modal when they
 *  differ. `null` means the user has never been shown the modal — treated
 *  as a fresh install (no popup, just records the current version). */
export interface WhatsNewConfig {
  last_seen_version: string | null;
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
