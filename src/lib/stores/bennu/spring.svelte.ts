/**
 * Framework-extension store — the frontend half of the `bennu-ext` seam.
 *
 * Holds the per-project overview (which extensions are active, their headline counts, the
 * property files) plus a small cache of catalog rows, so a panel that is opened, closed
 * and reopened doesn't re-ask the backend for a list that hasn't changed.
 *
 * Named for the seam, not for Spring: the catalogs are keyed by kind and the backend
 * decides which extension answers, so a second framework needs new panels, not a new
 * store.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { SvelteMap } from 'svelte/reactivity';
import {
  extCatalog,
  extOverview,
  extRefresh,
  setSpringPropertyFile,
  type ExtEntry,
  type ExtOverview,
  type PropertyFileInfo,
} from '$lib/ipc/bennu/ext';

function createSpringStore() {
  let overview = $state<ExtOverview | null>(null);
  let overviewRoot: string | null = null;
  let loadingOverview = $state(false);

  // kind → rows, for the project in `overviewRoot`. Cleared whenever the project changes
  // or the model is rebuilt, so a stale list can never outlive what it describes.
  const catalogs = new SvelteMap<string, ExtEntry[]>();
  const loadingKinds = new SvelteMap<string, boolean>();

  function invalidate() {
    catalogs.clear();
    loadingKinds.clear();
  }

  return {
    /** Whether ANY framework extension applies to the open project — the gate every
     *  framework palette entry and panel is hidden behind. */
    get available() {
      return (overview?.extensions.length ?? 0) > 0;
    },
    /** Whether the active extensions have finished building their model. */
    get ready() {
      return overview?.ready ?? false;
    },
    get overview() {
      return overview;
    },
    get loadingOverview() {
      return loadingOverview;
    },
    get propertyFiles(): PropertyFileInfo[] {
      return overview?.property_files ?? [];
    },
    get activePropertyFile(): string | null {
      return overview?.active_property_file ?? null;
    },
    /** Headline counts (Beans / Endpoints / …), in backend order. */
    get stats() {
      return overview?.stats ?? [];
    },

    /** Rows of `kind`, or `[]` until {@link loadCatalog} has fetched them. */
    rows(kind: string): ExtEntry[] {
      return catalogs.get(kind) ?? [];
    },
    isLoading(kind: string): boolean {
      return loadingKinds.get(kind) === true;
    },

    /** Fetch the overview for `root`. A repeat call for the same project is a no-op
     *  unless `force` — the panels call this on mount and the window calls it with
     *  `force` after the index rebuilds. */
    async loadOverview(root: string, force = false) {
      if (!force && overviewRoot === root && overview) return;
      if (overviewRoot !== root) invalidate();
      overviewRoot = root;
      loadingOverview = true;
      try {
        overview = await extOverview(root);
      } catch {
        // The backend may not have this domain (older process) — behave as "no framework
        // here" rather than surfacing an error for a feature the user never asked for.
        overview = null;
      } finally {
        loadingOverview = false;
      }
    },

    /** Fetch one catalog's rows, unless they are already cached. */
    async loadCatalog(root: string, kind: string, force = false) {
      if (!force && catalogs.has(kind)) return;
      loadingKinds.set(kind, true);
      try {
        catalogs.set(kind, await extCatalog(root, kind));
      } catch {
        catalogs.set(kind, []);
      } finally {
        loadingKinds.set(kind, false);
      }
    },

    /** Rebuild the backend model, then re-read everything currently on screen. */
    async refresh(root: string) {
      try {
        await extRefresh(root);
      } catch {
        return;
      }
      const kinds = [...catalogs.keys()];
      invalidate();
      await this.loadOverview(root, true);
      await Promise.all(kinds.map((k) => this.loadCatalog(root, k, true)));
    },

    /** Pin which property file `${…}` placeholders resolve against (`null` clears it).
     *  Persisted per project by the backend; the overview is re-read so the picker and
     *  every hover agree immediately. */
    async setPropertyFile(root: string, file: string | null) {
      try {
        await setSpringPropertyFile(root, file);
      } catch {
        return;
      }
      // Values change, so any cached property rows are stale.
      catalogs.delete('spring.properties');
      await this.loadOverview(root, true);
    },

    reset() {
      overview = null;
      overviewRoot = null;
      invalidate();
    },
  };
}

export const springStore = createSpringStore();
