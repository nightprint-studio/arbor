/**
 * Bennu Tomcat link — the per-project association to a local Tomcat used for JSP hot-swap.
 *
 * Real persistence (unlike run-config's mock): the link lives in `<repo>/.arbor/config.toml`
 * `[bennu.tomcat]` on the BE. This rune store is a per-root cache — `load(root)` hydrates it from
 * `get_tomcat_config`, `save(root, cfg)` writes through `set_tomcat_config`. Keyed by project root
 * via SvelteMap so `configFor` / `isLinked` stay reactive across project switches.
 *
 * Store pattern (CLAUDE.md): private `$state`, returned getters + methods.
 */

import { SvelteMap } from 'svelte/reactivity';
import { getTomcatConfig, setTomcatConfig, type TomcatConfig } from '$lib/ipc/bennu/tomcat';

/** An unlinked default. */
export function emptyTomcatConfig(): TomcatConfig {
  return { tomcat_root: '', webapp_name: '' };
}

function createTomcatStore() {
  const configs = new SvelteMap<string, TomcatConfig>();
  // Roots whose config has been fetched (so `configFor` can distinguish "empty" from "not loaded").
  const loaded = new SvelteMap<string, boolean>();

  return {
    /** The cached link for `root` (the empty default until {@link load} resolves). */
    configFor(root: string): TomcatConfig {
      return configs.get(root) ?? emptyTomcatConfig();
    },

    /** Whether `root` has a Tomcat root set (a hot-swap can be attempted). */
    isLinked(root: string): boolean {
      return (configs.get(root)?.tomcat_root ?? '').trim().length > 0;
    },

    /** Whether `root`'s config has been fetched from the BE at least once. */
    isLoaded(root: string): boolean {
      return loaded.get(root) === true;
    },

    /** Hydrate the cache for `root` from the BE (idempotent — safe to call on project open). */
    async load(root: string): Promise<TomcatConfig> {
      try {
        const cfg = await getTomcatConfig(root);
        configs.set(root, cfg);
        loaded.set(root, true);
        return cfg;
      } catch {
        const cfg = emptyTomcatConfig();
        configs.set(root, cfg);
        loaded.set(root, true);
        return cfg;
      }
    },

    /** Persist `cfg` for `root` (write-through) and update the cache. */
    async save(root: string, cfg: TomcatConfig): Promise<void> {
      await setTomcatConfig(root, cfg);
      configs.set(root, { ...cfg });
      loaded.set(root, true);
    },
  };
}

export const bennuTomcatStore = createTomcatStore();
