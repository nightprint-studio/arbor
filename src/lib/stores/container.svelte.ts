/**
 * Container store — Phase 2.
 *
 * Mirrors the backend `ContributionRegistry.containers` map and tracks the
 * currently-open container. The frontend listens to:
 *
 *   • arbor://container-open      { container_id }  — show ContributableModal
 *   • arbor://container-close     { container_id }  — hide it (ignored if it
 *                                                      wasn't the active one)
 *   • arbor://containers-changed                    — refetch defs (a plugin
 *                                                      registered or replaced
 *                                                      a container). Distinct
 *                                                      from
 *                                                      `contributions-changed`,
 *                                                      which fires for every
 *                                                      payload write — the
 *                                                      container registry is
 *                                                      a much smaller, slower-
 *                                                      moving slice.
 *   • arbor://plugins-reloaded                      — full refetch
 *
 * The store is the single source of truth for "is a container open and which
 * one"; consumers (AppShell) bind to `openContainerId`.
 */

import { listContainers } from '$lib/ipc/container';
import type { ContainerDef } from '$lib/types/contribution';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import { coalesceLatest } from '$lib/utils/coalesce';

function createContainerStore() {
  let _defs       = $state<Record<string, ContainerDef>>({});
  let _openId     = $state<string | null>(null);
  let _loaded     = $state(false);

  // Coalesce reloads — a plugin reload followed by N `contributions-changed`
  // bursts collapses to one refetch per frame.
  const reloadDefsCoalesced = coalesceLatest<void>(() => { void reloadDefs(); });

  async function reloadDefs() {
    try {
      const items = await listContainers();
      const next: Record<string, ContainerDef> = {};
      for (const d of items) next[d.key] = d;
      _defs = next;
    } catch { /* backend unavailable — keep previous state */ }
    _loaded = true;
  }

  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'arbor://container-open',
        handler: (e: { payload: { container_id?: string } }) => {
          const id = e.payload?.container_id;
          if (id) _openId = id;
        },
      },
      {
        event: 'arbor://container-close',
        handler: (e: { payload: { container_id?: string } }) => {
          // Match-by-id when the caller specifies one; an empty/missing id
          // means "close whatever is currently open" (used by the
          // arbor.ui.settings.close() shortcut, which has no key argument).
          const id = e.payload?.container_id;
          if (!id) { _openId = null; return; }
          if (_openId === id) _openId = null;
        },
      },
      {
        event: 'arbor://containers-changed',
        handler: () => reloadDefsCoalesced(),
      },
      {
        event: 'arbor://plugins-reloaded',
        handler: () => {
          // Backend wiped its registry on reload — clear local mirror, drop
          // any open modal whose backing def disappeared.
          _defs = {};
          _openId = null;
          reloadDefsCoalesced();
        },
      },
    ]);
  }

  /** Definition for a key, or null if the owner plugin no longer exists. */
  function getDef(key: string): ContainerDef | null {
    return _defs[key] ?? null;
  }

  /** All registered containers owned by the given plugin. */
  function defsForPlugin(pluginName: string): ContainerDef[] {
    return Object.values(_defs).filter(d => d.plugin_name === pluginName);
  }

  function open(key: string) { _openId = key; }
  function close() { _openId = null; }

  return {
    get loaded()          { return _loaded; },
    get openContainerId() { return _openId; },
    get defs()            { return _defs; },
    getDef,
    defsForPlugin,
    reloadDefs,
    setupListeners,
    open,
    close,
  };
}

export const containerStore = createContainerStore();
