/**
 * explorerProjects — the "Projects" data source for `FileExplorerModal`.
 *
 * `FileExplorerModal` is a product-agnostic widget: it *renders* a Projects
 * sidebar when given the data, but it must NOT know where that data comes from
 * (the git registry is a Corvus concept). So the caller passes a `source` — this
 * store is the canonical one for the git product.
 *
 * Each window is its own JS context (separate store instance). Whoever owns a
 * window that should surface projects calls `load()` once:
 *   • the Corvus main window (AppShell) — `load()`: reads through the running
 *     corvus-be, and mirrors the active repo tab,
 *   • the standalone Explorer window (ExplorerWindow) — `load({ local: true })`:
 *     reads through sitta-be (the git product isn't running there).
 * A product without projects (e.g. Merula) simply never calls `load()`, so the
 * source stays empty and no Projects section shows — or it passes its own
 * object of the same shape.
 *
 * The backend matters: `sitta-be` is spawned lazily only for explorer windows,
 * so the Corvus window must NOT read through it (it'd hit the "down" overlay and
 * come back empty). Conversely the standalone explorer has no corvus-be. Both
 * read the same `repos.json` / `workspaces.json`; only the routing differs.
 * Stays live on the `arbor://registry-changed` broadcast; `refresh()` re-fetches
 * on demand (e.g. once a lazily-spawned backend finally attaches).
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  listRegistryRepos, listWorkspaces,           // corvus-be (git product running)
  listRegistryReposLocal, listWorkspacesLocal, // sitta-be  (standalone explorer)
} from '$lib/ipc/corvus/workspace';
import type { RepoRegistryEntry, WorkspaceDef } from '$lib/types/corvus/workspace';

/** The shape `FileExplorerModal` consumes. Any product can supply its own
 *  implementation (getters keep it reactive). */
export interface ExplorerProjectsSource {
  readonly projects: RepoRegistryEntry[];
  readonly workspaces: WorkspaceDef[];
  readonly activeWorkspaceId: string | null;
  /** Path of the repo open in the active tab, so the sidebar can highlight it. */
  readonly activeRepoPath: string | null;
}

/** Context key a product's root sets (`setContext`) so every `FileExplorerModal`
 *  it hosts inherits the Projects source without per-call-site wiring. The
 *  explorer reads it with `getContext`; an explicit `source` prop overrides it.
 *  A product with no projects (Merula) simply never sets it → empty sidebar. */
export const EXPLORER_PROJECTS_KEY = Symbol('explorer-projects-source');

function createExplorerProjectsStore() {
  let projects = $state<RepoRegistryEntry[]>([]);
  let workspaces = $state<WorkspaceDef[]>([]);
  let activeWorkspaceId = $state<string | null>(null);
  let activeRepoPath = $state<string | null>(null);
  let unlisten: UnlistenFn | null = null;
  let loaded = false;
  // Which backend to route through: false → corvus-be (git product running),
  // true → sitta-be (standalone explorer). Set once by `load()`.
  let local = false;

  async function reload() {
    const listRepos = local ? listRegistryReposLocal : listRegistryRepos;
    const listWs    = local ? listWorkspacesLocal    : listWorkspaces;
    try {
      const [repos, snap] = await Promise.all([listRepos(), listWs()]);
      projects = repos;
      workspaces = snap.workspaces;
      activeWorkspaceId = snap.active_workspace_id;
    } catch { /* backend down / not this product — leave empty */ }
  }

  return {
    get projects() { return projects; },
    get workspaces() { return workspaces; },
    get activeWorkspaceId() { return activeWorkspaceId; },
    get activeRepoPath() { return activeRepoPath; },

    /** Highlight target — the git product feeds this from its active tab. */
    setActiveRepoPath(path: string | null) { activeRepoPath = path; },

    /** Load the registry + keep it live on `registry-changed`. Idempotent per
     *  window; safe to call from onMount. Pass `{ local: true }` from a window
     *  backed by sitta-be (the standalone explorer); omit it in the Corvus
     *  window so it reads through the running corvus-be. */
    async load(opts?: { local?: boolean }) {
      if (loaded) return;
      loaded = true;
      local = opts?.local ?? false;
      await reload();
      try {
        unlisten = await listen('arbor://registry-changed', () => { void reload(); });
      } catch { /* no dispatcher (rare) — one-shot load stands */ }
    },

    /** Force a re-fetch through the already-chosen backend. Use when a lazily
     *  spawned backend attaches after `load()` first ran (e.g. `sitta-be-up`),
     *  so an initial empty result gets filled in. No-op before `load()`. */
    refresh() { if (loaded) return reload(); },

    /** Tear down the registry listener (call from the owner's onDestroy). */
    dispose() { unlisten?.(); unlisten = null; loaded = false; },
  };
}

export const explorerProjects = createExplorerProjectsStore();
