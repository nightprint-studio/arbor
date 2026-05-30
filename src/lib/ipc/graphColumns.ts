import { invoke } from '@tauri-apps/api/core';
import type { GraphColumnsConfig } from '$lib/types/config';

export const getGraphColumns = () =>
  invoke<GraphColumnsConfig>('get_graph_columns');

export const setGraphColumns = (config: GraphColumnsConfig) =>
  invoke<void>('set_graph_columns', { config });
