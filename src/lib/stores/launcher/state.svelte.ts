/**
 * Shared launcher state: what products exist, which are running, their
 * versions, and the cross-product recents.
 *
 * Arbor has two homes — the launcher's own Canopy window (circuit-tree) and the
 * welcome page on the container's home tab — and they must agree on every one
 * of those facts. The wiring lives here so neither owns it: subscribe with
 * {@link launcherState.attach} and read the derived values.
 */
import { BASE, decorate, type DecoratedTool } from '$lib/components/launcher/canopy';
import { fetchInstalledVersions, fetchLatestVersions } from '$lib/components/launcher/versions';
import { closeProductWindow, listRunningProducts, onProductState } from '$lib/ipc/app';
import { getLauncherConfig, setLauncherCloseToTray } from '$lib/ipc/config';
import {
  listRecentProjects, forgetRecentProject, type RecentProject,
} from '$lib/ipc/recents';
import { windowModeStore } from '$lib/stores/window-mode.svelte';
import { openProduct, openProjectIn, restartProduct } from '$lib/utils/open-product';

const IDS = BASE.map((t) => t.id);

function createLauncherState() {
  let running   = $state<Set<string>>(new Set());
  let installed = $state<Record<string, string>>({});
  let latest    = $state<Record<string, string>>({});
  let closeToTray = $state<Record<string, boolean>>({});
  let recents   = $state<RecentProject[]>([]);

  const tools = $derived(BASE.map((t) => decorate(t, {
    running: running.has(t.id),
    installed: installed[t.id] ?? '—',
    latest: latest[t.id] ?? installed[t.id] ?? '—',
  })));

  async function loadRecents() {
    try { recents = await listRecentProjects(); } catch { recents = []; }
  }

  return {
    get tools()       { return tools; },
    get recents()     { return recents; },
    get closeToTray() { return closeToTray; },

    /**
     * Load everything and keep the running set live. Returns a disposer — call
     * it from an `$effect` so the product-state listener dies with the view.
     */
    attach(): () => void {
      let alive = true;
      void fetchInstalledVersions(IDS).then((v) => { if (alive) installed = v; });
      void fetchLatestVersions(IDS).then((v) => { if (alive) latest = v; });
      void listRunningProducts().then((l) => { if (alive) running = new Set(l); });
      void loadRecents();
      void getLauncherConfig().then((c) => {
        if (!alive) return;
        const map: Record<string, boolean> = {};
        for (const [k, v] of Object.entries(c.products ?? {})) map[k] = v.close_to_tray;
        closeToTray = map;
        // The same read seeds the window-mode store, so launching a product
        // doesn't pay for a second config round-trip.
        windowModeStore.hydrate(c.window_mode);
      });

      const unlisten = onProductState(({ id, running: r }) => {
        const next = new Set(running);
        if (r) next.add(id); else next.delete(id);
        running = next;
      });
      return () => { alive = false; void unlisten.then((fn) => fn()); };
    },

    refreshRecents: loadRecents,

    /** Launch or focus a product — window or container tab, per the setting. */
    launch(id: string) { return openProduct(id); },

    /** Open a recent: its product, started ON that project. */
    openRecent(r: RecentProject) { return openProjectIn(r.product, r.path); },

    async forgetRecent(r: RecentProject) {
      recents = recents.filter((x) => !(x.product === r.product && x.path === r.path));
      try { await forgetRecentProject(r.product, r.path); } catch { await loadRecents(); }
    },

    /**
     * Stop a product: close its windows **and** its container tab, whichever it is running as.
     *
     * Both live behind the one shell command, deliberately. A home can only reach the tabs of its
     * OWN window, and the Canopy launcher is not the container — so a Stop implemented on this
     * side could only ever stop half the products, which is exactly the half it could see.
     */
    stop(id: string) { return closeProductWindow(id); },

    /** Stop a product and start it again — see {@link restartProduct} for why the two homes share
     *  one implementation of it. */
    restart(id: string) { return restartProduct(id); },

    /** Re-check the release feed; today every product reports up to date. */
    async refreshVersions() { latest = await fetchLatestVersions(IDS); },

    async setCloseToTray(id: string, next: boolean) {
      const prev = closeToTray[id] ?? false;
      closeToTray = { ...closeToTray, [id]: next };
      try {
        await setLauncherCloseToTray(id, next);
      } catch (e) {
        closeToTray = { ...closeToTray, [id]: prev };
        throw e;
      }
    },
  };
}

export const launcherState = createLauncherState();
export type { DecoratedTool, RecentProject };
