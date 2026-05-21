/**
 * Reactive cache for cross-plugin contributions.
 *
 * The backend emits a single coalesced event — `arbor://contributions-changed`
 * — with `{ point }`. The store re-fetches that slice and replaces the
 * affected entry of the per-point index. Tree snapshots and custom icons live
 * in the same registry under canonical points (`arbor:tree-state` and
 * `arbor:icon`), so they are consumed via the same path; there is no parallel
 * cache.
 *
 * Public surface:
 *   • `forPoint(point)`     — items for a point, sorted by priority.
 *   • `tree(plugin, sidebar)` — projected `TreeSnapshot` for a tree-kind sidebar.
 *   • `ensureTree(...)`     — lazy fetch helper for the same.
 *   • `customIcon(ref)`     — `"plugin:<plugin>:<id>"` → raw SVG string.
 *   • `reloadAll() / reloadPoint() / setupListeners()` — wiring.
 *
 * Consumers that need typed shapes (sidebar sections, activity-bar entries,
 * panel content) use the parser utilities in `src/lib/contributions/*.ts`
 * applied to `forPoint(...)`. The `<Contribution>` primitive in
 * `src/lib/components/shared/Contribution.svelte` covers render-time
 * iteration with `when`/`disabled` filtering baked in.
 */
import { listPluginContributions } from '$lib/ipc/contribution';
import type {
  PluginContribution, TreeSnapshot, TreeNode,
} from '$lib/types/contribution';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import { coalesceLatest, coalesceLatestByKey } from '$lib/utils/coalesce';

const POINT_TREE_STATE = 'arbor:tree-state';
const POINT_ICON       = 'arbor:icon';

function createContributionStore() {
  // Per-point index. Reassigned (new Map) on every change so Svelte's
  // reference-tracking sees the update — `_byPoint.set(...)` would mutate in
  // place and miss reactivity.
  let _byPoint = $state<Map<string, PluginContribution[]>>(new Map());
  let _loaded  = $state(false);

  function _setPoint(point: string, items: PluginContribution[]): void {
    const next = new Map(_byPoint);
    if (items.length === 0) next.delete(point);
    else                    next.set(point, items);
    _byPoint = next;
  }

  async function reloadAll() {
    try {
      const items = await listPluginContributions();
      const next  = new Map<string, PluginContribution[]>();
      for (const c of items) {
        const bucket = next.get(c.point);
        if (bucket) bucket.push(c);
        else        next.set(c.point, [c]);
      }
      _byPoint = next;
    } catch { /* backend unavailable */ }
    _loaded = true;
  }

  async function reloadPoint(point: string) {
    try {
      _setPoint(point, await listPluginContributions(point));
    } catch { /* ignore */ }
  }

  // Coalesce contribution-change notifications.  Several plugins reloading
  // in burst (or a plugin reload + N contribution emits) collapse to one
  // refresh per affected point, plus at most one "reload all" — instead of
  // re-issuing the IPC + rebuilding the byPoint map N times.
  const reloadPointCoalesced = coalesceLatestByKey<string>(
    (point) => { void reloadPoint(point); },
    (p) => p,
  );
  const reloadAllCoalesced = coalesceLatest<void>(() => { void reloadAll(); });

  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'arbor://contributions-changed',
        handler: (e: { payload: { point?: string } }) => {
          if (e.payload?.point) reloadPointCoalesced(e.payload.point);
          else reloadAllCoalesced();
        },
      },
      {
        event: 'arbor://plugins-reloaded',
        handler: () => reloadAllCoalesced(),
      },
    ]);
  }

  /** Reactive read — list contributions for a given point, sorted by priority. */
  function forPoint(point: string): PluginContribution[] {
    return _byPoint.get(point) ?? [];
  }

  // ── Tree snapshots (point = "arbor:tree-state") ─────────────────────────

  /** Reactive read — current tree snapshot for a sidebar, or null. The payload
   *  carries `{ title, nodes, version }`; `plugin_name` and `sidebar_id` are
   *  the contribution's own identity, so we re-attach them to keep the public
   *  shape of `TreeSnapshot` stable. */
  function tree(pluginName: string, sidebarId: string): TreeSnapshot | null {
    const c = (_byPoint.get(POINT_TREE_STATE) ?? []).find(
      it => it.plugin_name === pluginName && it.item_id === sidebarId,
    );
    if (!c) return null;
    const p = c.payload as {
      title?:                       string;
      breadcrumb?:                  TreeSnapshot['breadcrumb'];
      breadcrumb_edit_action?:      string | null;
      breadcrumb_edit_placeholder?: string | null;
      nodes?:                       TreeNode[];
      version?:                     number;
    };
    return {
      plugin_name:                 pluginName,
      sidebar_id:                  sidebarId,
      title:                       p.title,
      breadcrumb:                  p.breadcrumb ?? [],
      breadcrumb_edit_action:      p.breadcrumb_edit_action ?? null,
      breadcrumb_edit_placeholder: p.breadcrumb_edit_placeholder ?? null,
      nodes:                       p.nodes ?? [],
      version:                     p.version ?? 0,
    };
  }

  /** Lazy ensure — kicks off a fetch of the tree-state slice if we don't have
   *  the requested snapshot yet. The function returns immediately; the
   *  reactive `tree(...)` getter picks up the snapshot when it lands. */
  function ensureTree(pluginName: string, sidebarId: string): void {
    if (tree(pluginName, sidebarId) !== null) return;
    reloadPoint(POINT_TREE_STATE);
  }

  // ── Custom icons (point = "arbor:icon") ─────────────────────────────────

  /** Resolve a `"plugin:<plugin>:<id>"` icon ref to its raw SVG, or null. */
  function customIcon(ref: string): string | null {
    if (!ref.startsWith('plugin:')) return null;
    const rest = ref.slice('plugin:'.length);
    const sep  = rest.indexOf(':');
    if (sep < 0) return null;
    const plugin = rest.slice(0, sep);
    const id     = rest.slice(sep + 1);
    const c = (_byPoint.get(POINT_ICON) ?? []).find(
      it => it.plugin_name === plugin && it.item_id === id,
    );
    return c ? ((c.payload as { svg?: string }).svg ?? null) : null;
  }

  return {
    get loaded() { return _loaded; },
    forPoint,
    tree,
    ensureTree,
    customIcon,
    reloadAll,
    reloadPoint,
    setupListeners,
  };
}

export const contributionStore = createContributionStore();
