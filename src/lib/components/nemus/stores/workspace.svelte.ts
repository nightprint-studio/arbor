/**
 * nemus workspace store — reactive mirror of the persisted nemus-window state
 * (`get_nemus_state` / `set_nemus_state`): the recent-projects list, the
 * last-opened project (reopened on launch) and the panel layout. NOT
 * localStorage and NOT the per-project nemus.toml — this is the dedicated nemus
 * window state file (Arbor hard rule #11). Mirrors the config/appearance store
 * shape: defaults are usable immediately, `load()` overwrites from disk, the
 * mutators persist.
 *
 * Layout persistence is debounced (~400ms): the panel toggles fire often (every
 * rail click / collapse), so we coalesce the writes instead of hammering the FS.
 */

import {
  getNemusState, setNemusState,
  type NemusLayoutState, type NemusWorkspaceState,
} from '$lib/ipc/nemus';

const RECENTS_CAP = 10;
const LAYOUT_PERSIST_DELAY = 400;

const DEFAULT_LAYOUT: NemusLayoutState = {
  left_panel:    'files',
  bottom_panel:  'console',
  right_panel:   'inspector',
  collapse_viz:  false,
  collapse_editor: false,
};

// Module-level debounce handle (the layout persist coalesces rapid toggles).
let layoutTimer: ReturnType<typeof setTimeout> | null = null;

function createWorkspaceStore() {
  let recentProjects = $state<string[]>([]);
  let lastProject    = $state<string | null>(null);
  let layout         = $state<NemusLayoutState>({ ...DEFAULT_LAYOUT });
  let loaded         = $state(false);

  function snapshot(): NemusWorkspaceState {
    return {
      recent_projects: [...recentProjects],
      last_project:    lastProject,
      layout:          { ...layout },
    };
  }

  function persist() { void setNemusState(snapshot()).catch(() => {}); }

  return {
    get recentProjects() { return recentProjects; },
    get lastProject()    { return lastProject; },
    get layout()         { return layout; },
    get loaded()         { return loaded; },

    /** Fetch the persisted state. On failure keep the defaults (first run /
     *  backend not ready). */
    async load() {
      try {
        const s = await getNemusState();
        recentProjects = s.recent_projects ?? [];
        lastProject    = s.last_project ?? null;
        layout         = { ...DEFAULT_LAYOUT, ...(s.layout ?? {}) };
        loaded = true;
      } catch {
        // Keep defaults; the next call retries.
      }
    },

    /** Record `path` as the most-recent project (dedupe, front, cap 10) and
     *  make it the last project. Persists immediately. */
    addRecent(path: string) {
      recentProjects = [path, ...recentProjects.filter(p => p !== path)].slice(0, RECENTS_CAP);
      lastProject = path;
      persist();
    },

    /** Set the project to reopen on launch (or clear it). Persists. */
    setLastProject(path: string | null) {
      if (lastProject === path) return;
      lastProject = path;
      persist();
    },

    /** Persist a new layout. Debounced (~400ms) — panel toggles fire often. */
    persistLayout(next: NemusLayoutState) {
      layout = { ...next };
      if (layoutTimer !== null) clearTimeout(layoutTimer);
      layoutTimer = setTimeout(() => { layoutTimer = null; persist(); }, LAYOUT_PERSIST_DELAY);
    },
  };
}

export const workspaceStore = createWorkspaceStore();
