import { invoke } from '@tauri-apps/api/core';
import type { PluginLogEntry } from '$lib/types/plugin-logs';

export const listPluginLogs = () =>
  invoke<PluginLogEntry[]>('list_plugin_logs');

export const clearPluginLogs = () =>
  invoke<void>('clear_plugin_logs');

export const clearPluginLogsByPipeline = (name: string) =>
  invoke<void>('clear_plugin_logs_by_pipeline', { name });
