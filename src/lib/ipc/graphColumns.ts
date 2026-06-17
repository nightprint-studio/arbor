import { platform } from './rpc';
import type { GraphColumnsConfig } from '$lib/types/config';

export const getGraphColumns = () =>
  platform<GraphColumnsConfig>('get_graph_columns');

export const setGraphColumns = (config: GraphColumnsConfig) =>
  platform<void>('set_graph_columns', { config });
