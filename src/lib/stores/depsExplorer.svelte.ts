/**
 * deps-explorer modal state.
 *
 * The plugin opens the modal by pushing a tree snapshot under sidebar id
 * `deps:<request_id>` (see `plugins/deps-explorer/main.lua`). Tree snapshots
 * flow through the unified contribution registry under the canonical point
 * `"arbor:tree-state"`, so we listen to `arbor://contributions-changed`,
 * filter that point, and recognise the `deps:*` namespace.
 *
 * Snapshot lifecycle:
 *   1. Plugin pushes initial { title, nodes:[loading sentinel] }   → modal opens
 *   2. Plugin pushes { title, nodes:[full tree] }                  → modal renders deps
 *   3. Plugin pushes { title, nodes:[full tree + maven-central] }  → modal patches
 *
 * Only one modal is open at a time. Re-clicking "Analyze dependencies" on
 * another module replaces the current request (the previous snapshot stays
 * in the contribution store cache; we just stop pointing at it).
 */
import { contributionStore } from '$lib/stores/contribution.svelte';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';

const PLUGIN     = 'deps-explorer';
const SID_PREFIX = 'deps:';
const POINT      = 'arbor:tree-state';

function createDepsExplorerStore() {
  let _currentSidebarId = $state<string | null>(null);
  // Sidebar ids the user has explicitly closed — subsequent contribution
  // events for these ids must NOT reopen the modal. The plugin keeps pushing
  // updates to the same id (initial loading snapshot, parsed tree, then the
  // Maven Central pass) and without this set, closing during the load phase
  // would have the modal pop back open as soon as the tree lands.
  const _dismissed = new Set<string>();

  function open(sidebarId: string) {
    _dismissed.delete(sidebarId);
    _currentSidebarId = sidebarId;
  }

  function close() {
    if (_currentSidebarId) _dismissed.add(_currentSidebarId);
    _currentSidebarId = null;
  }

  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'arbor://contributions-changed',
        handler: async (e: { payload: { point?: string } }) => {
          if (e.payload?.point !== POINT) return;
          // Make sure our scan sees the just-pushed snapshot. The contribution
          // store also reloads on this same event in parallel — duplicate
          // fetches are idempotent, this `await` is what guarantees we read
          // fresh data on this path.
          await contributionStore.reloadPoint(POINT);
          // Pick the best deps:* candidate the user hasn't dismissed and we
          // aren't already showing. When several land in the same coalesced
          // batch, the highest version wins.
          const candidates = contributionStore.forPoint(POINT)
            .filter(c =>
              c.plugin_name === PLUGIN &&
              c.item_id.startsWith(SID_PREFIX) &&
              !_dismissed.has(c.item_id) &&
              c.item_id !== _currentSidebarId,
            );
          if (candidates.length === 0) return;
          const best = candidates.reduce((acc, c) => {
            const v = (c.payload as { version?: number }).version ?? 0;
            const a = (acc.payload as { version?: number }).version ?? 0;
            return v > a ? c : acc;
          });
          _currentSidebarId = best.item_id;
        },
      },
      {
        event: 'arbor://plugins-reloaded',
        handler: () => {
          // Plugin registries are wiped — close the modal so we don't render
          // a snapshot the backend no longer believes in. Forget dismissed
          // ids too: they only refer to a previous plugin session.
          _currentSidebarId = null;
          _dismissed.clear();
        },
      },
    ]);
  }

  return {
    get pluginName() { return PLUGIN; },
    get currentSidebarId() { return _currentSidebarId; },
    get isOpen() { return _currentSidebarId !== null; },
    open,
    close,
    setupListeners,
  };
}

export const depsExplorerStore = createDepsExplorerStore();
