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
const RECENT_SOUNDS_CAP = 16;
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
  let favoriteSounds = $state<string[]>([]);
  let recentSounds   = $state<string[]>([]);
  let loaded         = $state(false);

  function snapshot(): NemusWorkspaceState {
    return {
      recent_projects: [...recentProjects],
      last_project:    lastProject,
      layout:          { ...layout },
      favorite_sounds: [...favoriteSounds],
      recent_sounds:   [...recentSounds],
    };
  }

  function persist() { void setNemusState(snapshot()).catch(() => {}); }

  return {
    get recentProjects() { return recentProjects; },
    get lastProject()    { return lastProject; },
    get layout()         { return layout; },
    get favoriteSounds() { return favoriteSounds; },
    get recentSounds()   { return recentSounds; },
    get loaded()         { return loaded; },

    /** Fetch the persisted state. On failure keep the defaults (first run /
     *  backend not ready). */
    async load() {
      try {
        const s = await getNemusState();
        recentProjects = s.recent_projects ?? [];
        lastProject    = s.last_project ?? null;
        layout         = { ...DEFAULT_LAYOUT, ...(s.layout ?? {}) };
        favoriteSounds = s.favorite_sounds ?? [];
        recentSounds   = s.recent_sounds ?? [];
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

    // ── Sound-bank favourites + recents (global, persisted) ──────────────────
    isFavoriteSound(name: string): boolean { return favoriteSounds.includes(name); },
    /** Toggle an instrument's favourite state. Persists. */
    toggleFavoriteSound(name: string) {
      favoriteSounds = favoriteSounds.includes(name)
        ? favoriteSounds.filter(n => n !== name)
        : [...favoriteSounds, name];
      persist();
    },
    /** Record an instrument as recently used (dedupe, front, cap). Persists. */
    addRecentSound(name: string) {
      recentSounds = [name, ...recentSounds.filter(n => n !== name)].slice(0, RECENT_SOUNDS_CAP);
      persist();
    },
  };
}

export const workspaceStore = createWorkspaceStore();
