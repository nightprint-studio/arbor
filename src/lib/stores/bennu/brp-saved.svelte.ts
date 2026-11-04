/**
 * The games and searches the user keeps for the Bevy Remote inspector.
 *
 * The backend owns the file (`bennu/brp.toml` in the profile) and normalises what it stores; this
 * store keeps the list for the UI and sends it back complete after every change, taking the
 * backend's answer as the truth so a trimmed or de-duplicated entry is never shown as saved when it
 * was not.
 */

import { brpSavedGet, brpSavedSet, type BrpSaved } from '$lib/ipc/bennu/brp';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

const EMPTY: BrpSaved = { endpoints: [], queries: [] };

function createBrpSavedStore() {
  let saved = $state<BrpSaved>(EMPTY);
  let loaded = false;

  async function load() {
    if (loaded) return;
    loaded = true;
    try {
      saved = await brpSavedGet();
    } catch {
      loaded = false;
    }
  }

  async function commit(next: BrpSaved) {
    try {
      saved = await brpSavedSet(next);
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
    }
  }

  const isEndpointSaved = (endpoint: string) =>
    saved.endpoints.some((e) => e.endpoint.toLowerCase() === endpoint.trim().toLowerCase());

  return {
    get endpoints() { return saved.endpoints; },
    get queries() { return saved.queries; },
    load,
    isEndpointSaved,

    /** Name defaults to the endpoint's `host:port`. Saving a known endpoint renames it. */
    async saveEndpoint(endpoint: string, name?: string) {
      const url = endpoint.trim();
      if (!url) return;
      const label = (name ?? '').trim() || url.replace(/^https?:\/\//, '').replace(/\/$/, '');
      const rest = saved.endpoints.filter((e) => e.endpoint.toLowerCase() !== url.toLowerCase());
      await commit({ ...saved, endpoints: [{ name: label, endpoint: url }, ...rest] });
    },
    async removeEndpoint(endpoint: string) {
      await commit({ ...saved, endpoints: saved.endpoints.filter((e) => e.endpoint !== endpoint) });
    },

    /** A query with the same name is replaced. */
    async saveQuery(name: string, search: string) {
      const label = name.trim();
      if (!label || !search.trim()) return;
      const rest = saved.queries.filter((q) => q.name.toLowerCase() !== label.toLowerCase());
      await commit({ ...saved, queries: [{ name: label, search: search.trim() }, ...rest] });
    },
    async removeQuery(name: string) {
      await commit({ ...saved, queries: saved.queries.filter((q) => q.name !== name) });
    },
  };
}

export const brpSavedStore = createBrpSavedStore();
