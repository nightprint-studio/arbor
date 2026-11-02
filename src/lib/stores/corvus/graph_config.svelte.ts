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

  // Legacy home of the squash-hints preference, before it moved to config.toml.
  const LEGACY_SQUASH_HINTS_KEY = 'arbor:experimental:squash-merge-hints';

  // One-shot migration: if the config never recorded a choice and the old localStorage key
  // exists, adopt it, persist it and drop the key so the user's earlier choice isn't lost.
  function migrateLegacySquashHints() {
    if (_config.squash_merge_hints != null) return;
    try {
      const legacy = localStorage.getItem(LEGACY_SQUASH_HINTS_KEY);
      if (legacy === null) return;
      _config = { ..._config, squash_merge_hints: legacy === 'true' };
      persist();
      localStorage.removeItem(LEGACY_SQUASH_HINTS_KEY);
    } catch { /* storage unavailable — nothing to migrate */ }
  }

  // Start loading immediately — not inside onMount or onMount-equivalent.
  // `whenReady` lets one-shot readers (the graph load) wait for the disk value instead of
  // racing it and silently seeing the default.
  const whenReady: Promise<void> = getGraphConfig()
    .then(cfg => { _config = cfg; migrateLegacySquashHints(); })
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

  function setSquashMergeHints(value: boolean) {
    if (_config.squash_merge_hints === value) return;
    _config = { ..._config, squash_merge_hints: value };
    persist();
  }

  return {
    get pageSize()            { return _config.page_size; },
    get showRemoteBranches()  { return _config.show_remote_branches; },
    get showTags()            { return _config.show_tags; },
    get paginate()            { return _config.paginate; },
    get ticketLinksEnabled()  { return _config.ticket_links_enabled; },
    /** Default OFF: the hints cost a network round-trip to the git provider. */
    get squashMergeHints()    { return _config.squash_merge_hints === true; },
    get ready()               { return _ready; },
    whenReady,
    setPaginate,
    setPageSize,
    setShowRemoteBranches,
    setShowTags,
    setTicketLinksEnabled,
    setSquashMergeHints,
  };
}

export const graphConfigStore = createGraphConfigStore();
