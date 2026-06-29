import { corvus } from './rpc';
import type { GraphColumnsConfig } from '$lib/types/config';

// Graph-column layout is a git-graph concern → owned by corvus-be (corvus/config.toml).
export const getGraphColumns = () =>
  corvus<GraphColumnsConfig>('get_graph_columns');

export const setGraphColumns = (config: GraphColumnsConfig) =>
  corvus<void>('set_graph_columns', { config });
