import { platform } from './rpc';
import type { PluginLogEntry, PluginLogLevel } from '$lib/types/plugin-logs';

export const listPluginLogs = () =>
  platform<PluginLogEntry[]>('list_plugin_logs');

export const clearPluginLogs = () =>
  platform<void>('clear_plugin_logs');

export const clearPluginLogsByPipeline = (name: string) =>
  platform<void>('clear_plugin_logs_by_pipeline', { name });

/** Append one entry to the plugin log ring buffer from the webview.
 *
 *  Prefer `reportPluginError` / `reportPluginWarning` in
 *  `$lib/utils/plugin-report` — they write the console line too, and a failure
 *  that reaches only one of the two is the thing they exist to stop. */
export const recordPluginLog = (level: PluginLogLevel, plugin: string, message: string) =>
  platform<void>('record_plugin_log', { level, plugin, message });
