/**
 * Which rows of Bennu's trees are open, per project root — the Project tree and the Maven tool
 * window, through one store.
 *
 * Lives outside both panels on purpose: a panel is unmounted whenever its rail button closes it,
 * and state held in the component was state reset every time the panel came back. Here it survives
 * that, a project switch and — because the project store writes it into the project's session in
 * `workspace.toml` — a restart.
 *
 * The keys, the persisted shape and pruning are pure and tested in
 * `components/bennu/tree-expansion.ts`. This file only holds them reactively and says when they
 * changed; it knows nothing about where they are written. The project store subscribes with
 * {@link onChange} and folds the write into its own debounced session save, so a burst of toggles
 * is one async IPC call rather than one per click.
 *
 * Rune store — private state, returned getters + methods (CLAUDE.md).
 */

import { SvelteMap } from 'svelte/reactivity';
import {
  absoluteId,
  decodeExpansion,
  encodeExpansion,
  relativeKey,
  scopedKey,
  staleKeys,
  type ExpansionScope,
  type PersistedExpansion,
} from '$lib/components/bennu/tree-expansion';

/** One tree's expansion for one project — what a panel hands to the rows it draws, so a row asks
 *  about its own key and never about roots or scopes. */
export interface ExpansionView {
  /** Whether the row is open; `fallback` is what the tree draws when nobody touched it. */
  isOpen(key: string, fallback?: boolean): boolean;
  setOpen(key: string, open: boolean, fallback?: boolean): void;
  toggle(key: string, fallback?: boolean): void;
}

const PROJECT_PREFIX = scopedKey('project', '');

function createTreeExpansionStore() {
  /** Project root (canonical, forward-slashed) → scoped key → open. Only overrides are stored. */
  const byRoot = new SvelteMap<string, SvelteMap<string, boolean>>();
  const listeners = new Set<() => void>();

  function notify() {
    for (const listener of listeners) listener();
  }

  /** Record one row. Returns whether anything changed. A row set back to its default drops its
   *  override rather than storing it, so the file carries only what differs. */
  function apply(root: string, scope: ExpansionScope, key: string, open: boolean, fallback: boolean): boolean {
    const id = scopedKey(scope, key);
    const state = byRoot.get(root);
    const had = state?.get(id);
    if (open === fallback) {
      if (had === undefined) return false;
      state!.delete(id);
      return true;
    }
    if (had === open) return false;
    if (state) state.set(id, open);
    else byRoot.set(root, new SvelteMap([[id, open]]));
    return true;
  }

  function isOpen(root: string, scope: ExpansionScope, key: string, fallback = false): boolean {
    return byRoot.get(root)?.get(scopedKey(scope, key)) ?? fallback;
  }

  function setOpen(root: string, scope: ExpansionScope, key: string, open: boolean, fallback = false) {
    if (apply(root, scope, key, open, fallback)) notify();
  }

  function setMany(root: string, scope: ExpansionScope, keys: Iterable<string>, open: boolean, fallback = false) {
    let changed = false;
    for (const key of keys) changed = apply(root, scope, key, open, fallback) || changed;
    if (changed) notify();
  }

  return {
    /** Be told after any user-visible change (not after {@link seed}). Returns the unsubscribe. */
    onChange(listener: () => void): () => void {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },

    /** Replace `root`'s state with what its session remembered. Silent: this IS the saved state. */
    seed(root: string, persisted: Partial<PersistedExpansion> | null | undefined) {
      byRoot.set(root, new SvelteMap(decodeExpansion(persisted)));
    },

    /** `root`'s state in its persisted form. */
    snapshot(root: string): PersistedExpansion {
      return encodeExpansion(byRoot.get(root) ?? []);
    },

    isOpen,
    setOpen,
    setMany,

    /** Drop every override of `scope` — Collapse all, on a tree whose default is collapsed. */
    clearScope(root: string, scope: ExpansionScope) {
      const state = byRoot.get(root);
      if (!state) return;
      const prefix = scopedKey(scope, '');
      const doomed = [...state.keys()].filter((key) => key.startsWith(prefix));
      for (const key of doomed) state.delete(key);
      if (doomed.length) notify();
    },

    /**
     * Forget the rows of `scope` that no longer exist. Call it only with the keys of a **complete,
     * current** load — never mid-load, and never from a reply a newer request superseded — or it
     * would forget rows that are merely not drawn yet.
     */
    prune(root: string, scope: ExpansionScope, live: ReadonlySet<string>) {
      const state = byRoot.get(root);
      if (!state) return;
      const stale = staleKeys(state.keys(), scope, live);
      for (const key of stale) state.delete(key);
      if (stale.length) notify();
    },

    /** One tree of one project, as a panel's rows use it. */
    view(root: string, scope: ExpansionScope): ExpansionView {
      return {
        isOpen: (key, fallback = false) => isOpen(root, scope, key, fallback),
        setOpen: (key, open, fallback = false) => setOpen(root, scope, key, open, fallback),
        toggle: (key, fallback = false) => setOpen(root, scope, key, !isOpen(root, scope, key, fallback), fallback),
      };
    },

    // ── The Project tree, in the absolute ids its rows carry ───────────────────
    /** The open folders of `root`'s Project tree — the controlled `expandedIds` of the shared Tree. */
    openPaths(root: string): Set<string> {
      const out = new Set<string>();
      for (const [key, open] of byRoot.get(root) ?? []) {
        if (open && key.startsWith(PROJECT_PREFIX)) out.add(absoluteId(root, key.slice(PROJECT_PREFIX.length)));
      }
      return out;
    },

    /** Open or close folders by absolute path. Paths outside `root` are ignored. */
    setPathsOpen(root: string, paths: Iterable<string>, open: boolean) {
      const keys: string[] = [];
      for (const path of paths) {
        const key = relativeKey(root, path);
        if (key) keys.push(key);
      }
      setMany(root, 'project', keys, open);
    },
  };
}

export const treeExpansionStore = createTreeExpansionStore();
