/**
 * Tracks tags created locally that have NOT yet been pushed to origin.
 *
 * Git itself has no notion of "local-only" tags — once fetched with `--tags`
 * a remote tag lands in `refs/tags/*` indistinguishable from one the user
 * created locally. Arbor persists the list per-repo in `.arbor/config.toml`
 * (handled by the backend `config_commands` for `local_only_tags`).
 *
 * This frontend store keeps a reactive in-memory cache keyed by tab id so
 * the Sidebar can render the "local" badge synchronously, plus async
 * helpers that mutate the backing TOML file and refresh the cache.
 */

import { invoke } from '@tauri-apps/api/core';

const _cache = $state<Record<string, Set<string>>>({});

async function _refresh(tabId: string): Promise<Set<string>> {
  try {
    const list = await invoke<string[]>('list_local_only_tags', { tabId });
    const set = new Set(list);
    _cache[tabId] = set;
    return set;
  } catch {
    if (!(tabId in _cache)) _cache[tabId] = new Set();
    return _cache[tabId];
  }
}

export const localTagTracker = {
  /** Reactive read — returns the currently-cached set for this tab.
   *  Call `load(tabId)` once on mount to populate it. */
  get(tabId: string): Set<string> {
    return _cache[tabId] ?? new Set();
  },

  /** Reactive read — true if the named tag is flagged local-only. */
  isLocal(tabId: string, name: string): boolean {
    return (_cache[tabId] ?? new Set()).has(name);
  },

  /** Load (or refresh) the cache from `.arbor/config.toml`. */
  async load(tabId: string): Promise<void> {
    await _refresh(tabId);
  },

  /** Mark a tag as locally-created and not-yet-pushed. */
  async markLocal(tabId: string, name: string): Promise<void> {
    try {
      await invoke<void>('mark_tag_local', { tabId, name });
    } finally {
      await _refresh(tabId);
    }
  },

  /** Mark a tag as pushed (or removed) — drops the "local" badge. */
  async markPushed(tabId: string, name: string): Promise<void> {
    try {
      await invoke<void>('mark_tag_pushed', { tabId, name });
    } finally {
      await _refresh(tabId);
    }
  },
};
