import { platform } from './rpc';
import type { PluginLogEntry } from '$lib/types/plugin-logs';

export const listPluginLogs = () =>
  platform<PluginLogEntry[]>('list_plugin_logs');

export const clearPluginLogs = () =>
  platform<void>('clear_plugin_logs');

export const clearPluginLogsByPipeline = (name: string) =>
  platform<void>('clear_plugin_logs_by_pipeline', { name });
