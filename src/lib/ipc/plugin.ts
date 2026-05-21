import { invoke } from '@tauri-apps/api/core';
import type { PluginManifest, PluginInfo } from '../types/plugin';

export const listPlugins = () =>
  invoke<PluginManifest[]>('list_plugins');

export const reloadPlugins = () =>
  invoke<void>('reload_plugins');

/** Master kill-switch — read whether the plugin system is enabled at all. */
export const getPluginsEnabled = () =>
  invoke<boolean>('get_plugins_enabled');

/**
 * Master kill-switch — turn the plugin system on/off.
 *  - `true`: backend re-discovers and loads every plugin from disk.
 *  - `false`: backend cancels jobs, fires on_plugin_unload, drops the runtime.
 * Both branches emit `arbor://plugins-reloaded` so listeners refresh.
 */
export const setPluginsEnabled = (enabled: boolean) =>
  invoke<void>('set_plugins_enabled', { enabled });

export const execHook = (hook: string, contextJson: string) =>
  invoke<void>('exec_hook', { hook, contextJson });

/** Fire a specific action on a specific plugin (called by the frontend when user interacts with plugin-registered UI). */
export const firePluginAction = (pluginName: string, action: string, contextJson: string) =>
  invoke<void>('fire_plugin_action', { pluginName, action, contextJson });

/** Enable a plugin by name (persisted across restarts; starts its scheduler if any). */
export const enablePlugin = (name: string) =>
  invoke<void>('enable_plugin', { name });

/** Disable a plugin by name (persisted across restarts; cancels its scheduler). */
export const disablePlugin = (name: string) =>
  invoke<void>('disable_plugin', { name });

/**
 * Permanently uninstall a plugin. Removes the plugin folder, its global
 * plugin_data, its persisted enable-state, and every per-repo
 * `.arbor/plugins/<name>/` (across open tabs + the workspace registry).
 * Returns a list of non-fatal warnings (paths that couldn't be deleted).
 */
export const deletePlugin = (name: string) =>
  invoke<string[]>('delete_plugin', { name });

/** List all loaded plugins with their enabled state and scheduler info. */
export const listPluginInfo = () =>
  invoke<PluginInfo[]>('list_plugin_info');

export interface DepGraphEdge {
  name:     string;
  version:  string;
  optional: boolean;
  /** True when the declared version requirement isn't satisfied by the loaded version. */
  unmet:    boolean;
}

export interface DepGraphNode {
  name:       string;
  version:    string;
  enabled:    boolean;
  depends_on: DepGraphEdge[];
  dependents: DepGraphEdge[];
  /** Dep-resolution error reported at load time, if any. */
  error:      string | null;
}

/** Return the plugin dependency graph (each plugin with its deps + dependents). */
export const pluginDepGraph = () =>
  invoke<DepGraphNode[]>('plugin_dep_graph');

/** Return the names of currently-enabled plugins that directly depend on `name`. */
export const pluginDependents = (name: string) =>
  invoke<string[]>('plugin_dependents', { name });

/** Start a specific scheduler action for a plugin. */
export const startPluginScheduler = (name: string, action: string) =>
  invoke<void>('start_plugin_scheduler', { name, action });

/** Stop a specific scheduler action for a plugin. */
export const stopPluginScheduler = (name: string, action: string) =>
  invoke<void>('stop_plugin_scheduler', { name, action });

/** Return all persisted settings for a plugin as a key→value map. */
export const pluginSettingsGet = (name: string) =>
  invoke<Record<string, unknown>>('plugin_settings_get', { name });

/** Overwrite all settings for a plugin with the provided key→value map. */
export const pluginSettingsSetAll = (name: string, values: Record<string, unknown>) =>
  invoke<void>('plugin_settings_set_all', { name, values });

/** Notify the backend whether the app window currently has focus.
 *  Focus-gated schedulers (only_when_focused = true) skip firing while this is false. */
export const setAppFocus = (focused: boolean) =>
  invoke<void>('set_app_focus', { focused });

/** Tell the backend which tab is currently active so arbor.repo.fetch_active_tab() works. */
export const setActiveTab = (tabId: string | null) =>
  invoke<void>('set_active_tab', { tabId });

// ── Plugin import / export (zip bundles) ───────────────────────────────────

/**
 * Form payload for `export_plugin_template`. Keys mirror the Rust struct
 * `ExportPluginTemplateOpts` exactly (Tauri converts camelCase ↔ snake_case
 * for top-level command argument names but NOT for fields inside a serde
 * payload, so we keep snake_case here).
 */
export interface ExportPluginTemplateOpts {
  // Identity
  name:        string;
  version:     string;
  description: string;
  author:      string;
  license?:    string | null;
  repository?: string | null;
  keywords:    string[];

  // Permissions
  fs:                   'none' | 'read' | 'write';
  fs_scope:             string[];
  git:                  'none' | 'read' | 'write' | 'history_rewrite';
  terminal:             'none' | 'commands' | 'any';
  terminal_scope:       string[];
  network:              string[];
  env_read:             boolean;
  issues:               'none' | 'read' | 'write';
  toolchain:            'none' | 'read' | 'write';
  service_export:       boolean;
  service_call:         boolean;
  settings_read_others: boolean;

  // Hooks
  hook_on_plugin_load:  boolean;
  hook_on_repo_open:    boolean;
  hook_on_repo_close:   boolean;
  hook_on_tab_switch:   boolean;
  hook_on_commit:       boolean;
  hook_on_push:         boolean;
  hook_on_pull:         boolean;
  hook_on_fetch:        boolean;
  hook_on_checkout:     boolean;
  hook_on_branch_create: boolean;
  hook_on_branch_delete: boolean;
  hook_on_mr_opened:    boolean;
  hook_on_mr_merged:    boolean;

  // Scheduler
  include_scheduler: boolean;

  // Snippets / recipes
  snippet_command:        boolean;
  snippet_keybinding:     boolean;
  snippet_settings_panel: boolean;
  snippet_modal:          boolean;
  snippet_action_toolbar: boolean;
  snippet_sidebar:        boolean;
  snippet_notification:   boolean;
  snippet_job_spawn:      boolean;
  snippet_scheduler:      boolean;
  snippet_http_get:       boolean;
}

export interface ImportPluginResult {
  plugin_name: string;
  plugin_dir:  string;
  files:       number;
}

/**
 * Build a plugin template zip from the modal form and write it directly to
 * `targetPath` (the path returned by Arbor's FilePickerModal in save mode).
 * Returns the absolute path that was actually written — when the picker
 * handed back a directory the backend appends `<slug>.zip`.
 */
export const exportPluginTemplateToPath = (opts: ExportPluginTemplateOpts, targetPath: string) =>
  invoke<string>('export_plugin_template_to_path', { opts, targetPath });

/** Install a plugin zip (already in memory) into the user's plugins directory. */
export const importPluginZip = (zipBytes: Uint8Array) =>
  invoke<ImportPluginResult>('import_plugin_zip', { zipBytes: Array.from(zipBytes) });

/** Install a plugin zip by absolute path — backend reads the file itself. */
export const importPluginZipFromPath = (path: string) =>
  invoke<ImportPluginResult>('import_plugin_zip_from_path', { path });
