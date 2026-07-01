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
  ticket_links_enabled: true,
};

function createGraphConfigStore() {
  let _config = $state<GraphConfig>({ ...DEFAULTS });
  let _ready  = $state(false);

  // Start loading immediately — not inside onMount or onMount-equivalent.
  getGraphConfig()
    .then(cfg => { _config = cfg; })
    .catch(() => { /* keep defaults */ })
    .finally(() => { _ready = true; });

  function persist() {
    void setGraphConfig(_config).catch(() => {});
  }

  async function setPaginate(value: boolean): Promise<void> {
    _config = { ..._config, paginate: value };
    await setGraphConfig(_config);
  }

  function setPageSize(value: number) {
    if (!Number.isFinite(value)) return;
    const next = Math.max(100, Math.min(2000, Math.floor(value)));
    if (next === _config.page_size) return;
    _config = { ..._config, page_size: next };
    persist();
  }

  function setShowRemoteBranches(value: boolean) {
    if (_config.show_remote_branches === value) return;
    _config = { ..._config, show_remote_branches: value };
    persist();
  }

  function setShowTags(value: boolean) {
    if (_config.show_tags === value) return;
    _config = { ..._config, show_tags: value };
    persist();
  }

  function setTicketLinksEnabled(value: boolean) {
    if (_config.ticket_links_enabled === value) return;
    _config = { ..._config, ticket_links_enabled: value };
    persist();
  }

  return {
    get pageSize()            { return _config.page_size; },
    get showRemoteBranches()  { return _config.show_remote_branches; },
    get showTags()            { return _config.show_tags; },
    get paginate()            { return _config.paginate; },
    get ticketLinksEnabled()  { return _config.ticket_links_enabled; },
    get ready()               { return _ready; },
    setPaginate,
    setPageSize,
    setShowRemoteBranches,
    setShowTags,
    setTicketLinksEnabled,
  };
}

export const graphConfigStore = createGraphConfigStore();
