/**
 * Singleton store for graph configuration (persisted in config.toml).
 *
 * Kicks off an IPC load immediately at module import time so the value is
 * ready (or very close to ready) by the time CommitGraph's first $effect runs.
 */
import { getGraphConfig, setGraphConfig } from '$lib/ipc/config';
import type { GraphConfig } from '$lib/types/config';

const DEFAULTS: GraphConfig = {
  page_size: 500,
  show_remote_branches: true,
  show_tags: true,
  paginate: true,
};

function createGraphConfigStore() {
  let _config = $state<GraphConfig>({ ...DEFAULTS });
  let _ready  = $state(false);

  // Start loading immediately — not inside onMount or onMount-equivalent.
  getGraphConfig()
    .then(cfg => { _config = cfg; })
    .catch(() => { /* keep defaults */ })
    .finally(() => { _ready = true; });

  async function setPaginate(value: boolean): Promise<void> {
    _config = { ..._config, paginate: value };
    await setGraphConfig(_config);
  }

  return {
    get paginate() { return _config.paginate; },
    get ready()    { return _ready; },
    setPaginate,
  };
}

export const graphConfigStore = createGraphConfigStore();
