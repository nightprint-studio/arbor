import { invoke } from '@tauri-apps/api/core';
import { platform } from './rpc';
import { host } from './host';
import { reportPluginError } from '$lib/utils/plugin-report';
import type { PluginManifest, PluginInfo, ExtensionsReport } from '../types/plugin';

/**
 * Run a plugin's own code and make sure a failure is heard.
 *
 * The three entry points below (`execHook`, `firePluginAction`, `fireCommand`)
 * are the only ways the frontend asks a plugin to *do* something, and their
 * callers are overwhelmingly fire-and-forget — `.catch(() => {})` on a click
 * handler, because there is nothing sensible for a click to do about it. That
 * left every one of those failures with no reader at all.
 *
 * Reporting here rather than at each call site is the difference between one
 * place and twenty: a new panel, a new node type, a new context menu inherits it
 * by construction. The promise still rejects — a caller that does have something
 * to say about a failure keeps the chance to say it.
 */
function reporting<T>(plugin: string, what: string, p: Promise<T>): Promise<T> {
  return p.catch((e) => {
    reportPluginError(plugin, what, e);
    throw e;
  });
}

export const listPlugins = () =>
  platform<PluginManifest[]>('list_plugins');

export const reloadPlugins = () =>
  host<void>('reload_plugins');

/**
 * The wasm extensions installed packages provide, plus everything wrong with them.
 *
 * Separate from `listPlugins` because they are a different kind of thing: a plugin calls
 * Arbor's API, an extension implements an interface Arbor calls into — so it has no hooks, no
 * settings panel and no on/off of its own.
 */
export const listExtensions = () =>
  platform<ExtensionsReport>('list_extensions');

/**
 * Bring one extension up and let it go again, reporting whether it came up.
 *
 * Resolves on success and rejects with the reason on failure. The point is the bringing up:
 * instantiating exercises the whole chain — the module compiles, its imports resolve against
 * what the host offers, its exports match the world it claims — and every one of those
 * failures is otherwise invisible until somebody opens a file and nothing happens.
 */
export const probeExtension = (iface: string, version: number, id: string) =>
  platform<void>('probe_extension', { interface: iface, version, id });

/** Master kill-switch — read whether the plugin system is enabled at all. */
export const getPluginsEnabled = () =>
  platform<boolean>('get_plugins_enabled');

/**
 * Master kill-switch — turn the plugin system on/off.
 *  - `true`: backend re-discovers and loads every plugin from disk.
 *  - `false`: backend cancels jobs, fires on_plugin_unload, drops the runtime.
 * Both branches emit `arbor://plugins-reloaded` so listeners refresh.
 */
export const setPluginsEnabled = (enabled: boolean) =>
  host<void>('set_plugins_enabled', { enabled });

/** Fire a hook at every subscriber. Not attributable to one plugin, so a
 *  failure here is the host's — it reaches the console the ordinary way. */
export const execHook = (hook: string, contextJson: string) =>
  host<void>('exec_hook', { hook, context_json: contextJson });

/** Fire a specific action on a specific plugin (called by the frontend when user interacts with plugin-registered UI). */
export const firePluginAction = (pluginName: string, action: string, contextJson: string) =>
  reporting(
    pluginName, `action '${action}' failed`,
    host<void>('fire_plugin_action', { plugin_name: pluginName, action, context_json: contextJson }),
  );

/**
 * Invoke a registered command on behalf of `callerPlugin` (declarative
 * `kind = "command"` dispatch). The backend enforces the `command_invoke`
 * permission + the command's `required` tier; rejections surface as an error.
 * `args` is the static dispatch-slot data; `contextJson` is the node payload.
 */
export const fireCommand = (
  callerPlugin: string,
  id: string,
  args: unknown,
  contextJson: string,
) => reporting(
  callerPlugin, `command '${id}' failed`,
  host<void>('fire_command', { caller_plugin: callerPlugin, id, args, context_json: contextJson }),
);

/**
 * Enable a plugin. Returns the ordered list of plugins that were actually
 * enabled — required deps first, target last. The backend refuses to enable
 * when a required dependency is missing or unloadable; call `pluginEnablePreview`
 * first to detect blockers and prompt the user when the cascade is non-trivial.
 */
export const enablePlugin = (name: string) =>
  host<string[]>('enable_plugin', { name });

/**
 * Disable a plugin. Returns the ordered list of plugins that were disabled
 * — every transitively-required dependent, leaves-first, with `name` last.
 * Call `pluginDisablePreview` first when you need to show a confirmation.
 */
export const disablePlugin = (name: string) =>
  host<string[]>('disable_plugin', { name });

/** One blocker preventing a plugin's enable cascade from running. */
export interface EnableBlocker {
  name:        string;
  version_req: string;
  /** Human-readable reason: "not installed" | "version mismatch: …" | "failed to load: …". */
  reason:      string;
}

export interface EnablePreview {
  /** Ordered list of plugins that would be enabled (deps first, target last). */
  plan:     string[];
  /** Required deps that are missing / unloadable / version-incompatible. */
  blockers: EnableBlocker[];
}

/** Preview an enable cascade — used to drive the confirmation modal. */
export const pluginEnablePreview = (name: string) =>
  host<EnablePreview>('plugin_enable_preview', { name });

/** Preview a disable cascade — every transitively-required dependent. */
export const pluginDisablePreview = (name: string) =>
  host<string[]>('plugin_disable_preview', { name });

/**
 * Permanently uninstall a plugin. Removes the plugin folder, its global
 * plugin_data, its persisted enable-state, and every per-repo
 * `.arbor/plugins/<name>/` (across open tabs + the workspace registry).
 * Returns a list of non-fatal warnings (paths that couldn't be deleted).
 */
export const deletePlugin = (name: string) =>
  platform<string[]>('delete_plugin', { name });

/** List all loaded plugins with their enabled state and scheduler info. */
export const listPluginInfo = () =>
  host<PluginInfo[]>('list_plugin_info');

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
  host<DepGraphNode[]>('plugin_dep_graph');

/** Return the names of currently-enabled plugins that directly depend on `name`. */
export const pluginDependents = (name: string) =>
  host<string[]>('plugin_dependents', { name });

/** Start a specific scheduler action for a plugin. */
export const startPluginScheduler = (name: string, action: string) =>
  host<void>('start_plugin_scheduler', { name, action });

/** Stop a specific scheduler action for a plugin. */
export const stopPluginScheduler = (name: string, action: string) =>
  host<void>('stop_plugin_scheduler', { name, action });

/** Return all persisted settings for a plugin as a key→value map. */
export const pluginSettingsGet = (name: string) =>
  host<Record<string, unknown>>('plugin_settings_get', { name });

/** Overwrite all settings for a plugin with the provided key→value map. */
export const pluginSettingsSetAll = (name: string, values: Record<string, unknown>) =>
  host<void>('plugin_settings_set_all', { name, values });

/** Notify the backend whether the app window currently has focus.
 *  Focus-gated schedulers (only_when_focused = true) skip firing while this is false. */
export const setAppFocus = (focused: boolean) =>
  invoke<void>('set_app_focus', { focused });

/** Tell the backend which tab is currently active so arbor.repo.fetch_active_tab() works. */
export const setActiveTab = (tabId: string | null) =>
  host<void>('set_active_tab', { tab_id: tabId });

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
 * `targetPath` (the path returned by Arbor's FileExplorerModal in save mode).
 * Returns the absolute path that was actually written — when the picker
 * handed back a directory the backend appends `<slug>.zip`.
 */
export const exportPluginTemplateToPath = (opts: ExportPluginTemplateOpts, targetPath: string) =>
  platform<string>('export_plugin_template_to_path', { opts, target_path: targetPath });

/** Install a plugin zip (already in memory) into the user's plugins directory. */
export const importPluginZip = (zipBytes: Uint8Array) =>
  platform<ImportPluginResult>('import_plugin_zip', { zip_bytes: Array.from(zipBytes) });

/** Install a plugin zip by absolute path — backend reads the file itself. */
export const importPluginZipFromPath = (path: string) =>
  platform<ImportPluginResult>('import_plugin_zip_from_path', { path });

/**
 * Resolve the on-disk folder of a discovered plugin by name. Errors when no
 * installed plugin matches. The folder name on disk can differ from the
 * manifest's `name` (zip imports preserve the archive root), so this is the
 * only reliable way to map name → path from the FE.
 */
export const getInstalledPluginPath = (name: string) =>
  platform<string>('get_installed_plugin_path', { name });
