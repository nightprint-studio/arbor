/**
 * The tabbed container's open products.
 *
 * Only meaningful inside the `workspace` window (see `WorkspaceContainer`);
 * elsewhere `inContainer` stays false and every consumer — notably the product
 * title bars, which render the tab strip — renders nothing.
 *
 * **One tab per product, by construction.** Each product keeps its state in
 * module-level stores, and a window has exactly one copy of those, so two
 * Corvus tabs in one window would share a single repository state and mirror
 * each other. A second instance of a product therefore belongs in its own
 * window — which is what `detach` does, and what multi-monitor users want
 * anyway. Corvus already stacks repositories in its own tab bar, so nothing is
 * lost.
 */

import { workspaceTabOpened, workspaceTabClosed } from '$lib/ipc/window';

/** A product that can live in a container tab. */
export type SurfaceId = 'home' | 'corvus' | 'bennu' | 'merula';

export interface SurfaceDef {
  id:    SurfaceId;
  label: string;
}

/** Every tabbable surface, in the order the tab strip shows them. `home` is the
 *  welcome page — the new-tab page, not a product. */
export const SURFACES: SurfaceDef[] = [
  { id: 'home',   label: 'Welcome' },
  { id: 'corvus', label: 'Corvus' },
  { id: 'bennu',  label: 'Bennu'  },
  { id: 'merula', label: 'merula' },
];

export function surfaceDef(id: SurfaceId): SurfaceDef {
  return SURFACES.find((s) => s.id === id) ?? { id, label: id };
}

/** Which tabs were open last time. Session-shaped UI state (like panel ratios),
 *  so it belongs in localStorage rather than the config file. */
const STORAGE_KEY = 'arbor:workspace-tabs';

interface Persisted { tabs: SurfaceId[]; active: SurfaceId | null }

function load(): Persisted {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null') as Persisted | null;
    if (!raw || !Array.isArray(raw.tabs)) return { tabs: ['home'], active: 'home' };
    const tabs = raw.tabs.filter((t): t is SurfaceId => SURFACES.some((s) => s.id === t));
    if (!tabs.length) return { tabs: ['home'], active: 'home' };
    return { tabs, active: tabs.includes(raw.active as SurfaceId) ? raw.active : tabs[0] };
  } catch {
    return { tabs: ['home'], active: 'home' };
  }
}

function createSurfaceStore() {
  let inContainer = $state(false);
  let tabs   = $state<SurfaceId[]>([]);
  let active = $state<SurfaceId | null>(null);
  // A tab's shell is mounted the first time it's visited and then kept alive —
  // switching back must be instant, and re-mounting a product shell would
  // re-run its whole boot (config reads, backend handshakes, index loads).
  let mounted = $state<SurfaceId[]>([]);
  // Tabs whose backend is still coming up: opened, not yet mountable.
  let pending = $state<SurfaceId[]>([]);

  function persist() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ tabs, active }));
    } catch { /* quota / private mode */ }
  }

  /**
   * Mount a tab's shell, announcing the product to the shell the first time.
   *
   * A product in a tab needs the same setup as a product in its own window —
   * backend spawned, launcher node lit — and only the frontend knows a tab
   * exists, so it reports it. Restored tabs come through here too: they never
   * went through `openWorkspaceWindow`, so this is their only announcement.
   *
   * The shell is mounted only AFTER the backend is up. A product shell fires
   * its first backend call as it mounts and does not retry — mounting first
   * would answer it with `unknown command`, which is exactly how a tab used to
   * come up dead.
   */
  async function markMounted(id: SurfaceId) {
    if (mounted.includes(id) || pending.includes(id)) return;
    if (id === 'home') {
      mounted = [...mounted, id];
      return;
    }
    pending = [...pending, id];
    try {
      await workspaceTabOpened(id);
    } catch {
      // Backend refused to come up: mount anyway. A degraded product (its shell
      // reporting its own backend as down) beats a tab that never appears.
    }
    pending = pending.filter((p) => p !== id);
    if (tabs.includes(id)) mounted = [...mounted, id];
  }

  return {
    get inContainer() { return inContainer; },
    get tabs()   { return tabs; },
    get active() { return active; },
    get mounted() { return mounted; },

    /** True when `id` is the tab on screen — the gate a suspended shell reads. */
    isActive(id: SurfaceId) { return active === id; },

    /**
     * Should the shell of `id` react to a window-level event?
     *
     * Background tabs stay MOUNTED (their state must survive a tab switch), and
     * `display: none` hides them but does not unsubscribe their
     * `<svelte:window on…>` handlers — so without this gate every global
     * shortcut would fire in all open products at once. Always true outside the
     * container, where the window hosts exactly one product.
     */
    hasFocus(id: SurfaceId) { return !inContainer || active === id; },

    /** Called once by the container as it mounts; restores the last session. */
    enterContainer() {
      const restored = load();
      inContainer = true;
      tabs = restored.tabs;
      active = restored.active;
      if (active) void markMounted(active);
    },

    /** Open `id` if needed and bring it to the front. Idempotent. */
    show(id: SurfaceId) {
      const isNew = !tabs.includes(id);
      if (isNew) tabs = [...tabs, id];
      // Opening a product REPLACES the welcome page: it did its job, and a tab
      // that lingers behind every product is just a tab you keep closing. `+`
      // (or Ctrl+T) brings it back. Only on a genuine open — switching to a tab
      // that is already there must never close another one.
      if (isNew && id !== 'home' && tabs.includes('home')) {
        tabs = tabs.filter((t) => t !== 'home');
        mounted = mounted.filter((t) => t !== 'home');
      }
      active = id;
      void markMounted(id);
      persist();
    },

    /** Add the Canopy home tab and focus it — the container's "new tab". */
    openHome() { this.show('home'); },

    close(id: SurfaceId) {
      const idx = tabs.indexOf(id);
      if (idx === -1) return;
      const wasMounted = mounted.includes(id);
      tabs = tabs.filter((t) => t !== id);
      mounted = mounted.filter((t) => t !== id);
      // Closing a tab ends that product, exactly like closing its window: the
      // shell tears the backend down and clears the launcher node.
      if (wasMounted && id !== 'home') void workspaceTabClosed(id).catch(() => {});
      if (active === id) {
        // Focus the neighbour, like a browser: the tab to the right, else left.
        active = tabs[idx] ?? tabs[idx - 1] ?? null;
        if (active) void markMounted(active);
      }
      // Never leave the container empty and blank — fall back to the home tab.
      if (!tabs.length) {
        tabs = ['home'];
        active = 'home';
        void markMounted('home');
      }
      persist();
    },

    /** Step through the tabs (wrapping), for the next/previous shortcuts. */
    step(delta: number) {
      if (tabs.length < 2 || !active) return;
      const idx = (tabs.indexOf(active) + delta + tabs.length) % tabs.length;
      this.show(tabs[idx]);
    },

    /** Focus the nth tab (0-based) if it exists — the Ctrl+1…9 shortcuts. */
    showIndex(idx: number) {
      const id = tabs[idx];
      if (id) this.show(id);
    },
  };
}

export const surfaceStore = createSurfaceStore();
