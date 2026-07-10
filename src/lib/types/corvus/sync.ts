// Mirror of the corvus-be `sync` wire types (serde snake_case).
// Settings sync to a private git-provider repo — see `crates/products/corvus/be/src/sync`.

export interface SyncConfig {
  enabled: boolean;
  provider: string | null;
  repo_name: string | null;
  repo_full_name: string | null;
  clone_url: string | null;
  interval_secs: number;
  include_workspaces: boolean;
  include_settings: boolean;
  include_mods: boolean;
  include_plugin_data: boolean;
  plugin_data_cap_kb: number;
  last_push_at: number | null;
  last_pull_at: number | null;
  last_machine: string | null;
}

export interface SyncStatus {
  enabled: boolean;
  provider: string | null;
  repo_full_name: string | null;
  clone_url: string | null;
  interval_secs: number;
  include_workspaces: boolean;
  include_settings: boolean;
  include_mods: boolean;
  include_plugin_data: boolean;
  last_push_at: number | null;
  last_pull_at: number | null;
  last_machine: string | null;
  dirty: boolean;
  awaiting_pull: boolean;
}

// ── Pull plan / apply ────────────────────────────────────────────────────────

/** `new` (absent locally), `changed` (differs), or `same`. */
export type WsStatus = 'new' | 'changed' | 'same';

export interface PullWorkspaceItem {
  id: string;
  name: string;
  status: WsStatus;
  repo_count: number;
}

export interface PullSettingsItem {
  key: 'profile' | 'corvus';
  label: string;
  differs: boolean;
}

export interface PullModItem {
  name: string;
  version: string;
  installed: boolean;
  enabled: boolean;
}

export interface PullDataItem {
  name: string;
  differs: boolean;
}

export interface PullMissingRepo {
  remote_url: string;
  display_name: string;
}

export interface PullPlan {
  available: boolean;
  workspaces: PullWorkspaceItem[];
  settings: PullSettingsItem[];
  mods: PullModItem[];
  plugin_data: PullDataItem[];
  missing_repos: PullMissingRepo[];
}

export interface PullSelections {
  workspace_ids: string[];
  settings_keys: string[];
  plugin_data_names: string[];
}

export interface PullSummary {
  workspaces_applied: number;
  settings_applied: number;
  mods_enabled: number;
  plugin_data_applied: number;
  missing_repos: PullMissingRepo[];
  settings_reload_needed: boolean;
}
