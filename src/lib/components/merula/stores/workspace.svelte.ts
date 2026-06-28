/**
 * merula workspace store — reactive mirror of the persisted merula-window state
 * (`get_merula_state` / `set_merula_state`): the recent-projects list, the
 * last-opened project (reopened on launch) and the panel layout. NOT
 * localStorage and NOT the per-project merula.toml — this is the dedicated merula
 * window state file (Arbor hard rule #11). Mirrors the config/appearance store
 * shape: defaults are usable immediately, `load()` overwrites from disk, the
 * mutators persist.
 *
 * Layout persistence is debounced (~400ms): the panel toggles fire often (every
 * rail click / collapse), so we coalesce the writes instead of hammering the FS.
 */

import {
  getMerulaState, setMerulaState,
  type MerulaLayoutState, type MerulaWorkspaceState, type MerulaProjectWorkspace,
} from '$lib/ipc/merula';

const RECENTS_CAP = 10;
const RECENT_SOUNDS_CAP = 16;
const LAYOUT_PERSIST_DELAY = 400;

/** Neon-ish workspace accent palette (Arbor-style coloured groups), kept merula-local
 *  so a workspace colour is stable across themes. Indexed by `color_idx`. */
export const WORKSPACE_COLORS = [
  '#4ea6ff', '#3ddc97', '#ff9e3d', '#ff5d8f', '#b388ff', '#ffd23d', '#4dd2ff', '#d46bff',
];
export function workspaceColor(idx: number): string {
  return WORKSPACE_COLORS[((idx % WORKSPACE_COLORS.length) + WORKSPACE_COLORS.length) % WORKSPACE_COLORS.length];
}

const DEFAULT_LAYOUT: MerulaLayoutState = {
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
  let layout         = $state<MerulaLayoutState>({ ...DEFAULT_LAYOUT });
  let favoriteSounds = $state<string[]>([]);
  let recentSounds   = $state<string[]>([]);
  let workspaces     = $state<MerulaProjectWorkspace[]>([]);
  let activeWorkspace = $state<string | null>(null);
  let loaded         = $state(false);

  function snapshot(): MerulaWorkspaceState {
    return {
      recent_projects: [...recentProjects],
      last_project:    lastProject,
      layout:          { ...layout },
      favorite_sounds: [...favoriteSounds],
      recent_sounds:   [...recentSounds],
      workspaces:      workspaces.map(w => ({ ...w, project_paths: [...w.project_paths] })),
      active_workspace: activeWorkspace,
    };
  }

  function persist() { void setMerulaState(snapshot()).catch(() => {}); }

  return {
    get recentProjects() { return recentProjects; },
    get lastProject()    { return lastProject; },
    get layout()         { return layout; },
    get favoriteSounds() { return favoriteSounds; },
    get recentSounds()   { return recentSounds; },
    get workspaces()     { return workspaces; },
    get activeWorkspace() { return activeWorkspace; },
    /** The active workspace object, or null. */
    get activeWorkspaceObj(): MerulaProjectWorkspace | null {
      return workspaces.find(w => w.id === activeWorkspace) ?? null;
    },
    get loaded()         { return loaded; },

    /** Fetch the persisted state. On failure keep the defaults (first run /
     *  backend not ready). */
    async load() {
      try {
        const s = await getMerulaState();
        recentProjects = s.recent_projects ?? [];
        lastProject    = s.last_project ?? null;
        layout         = { ...DEFAULT_LAYOUT, ...(s.layout ?? {}) };
        favoriteSounds = s.favorite_sounds ?? [];
        recentSounds   = s.recent_sounds ?? [];
        workspaces     = s.workspaces ?? [];
        activeWorkspace = s.active_workspace ?? null;
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
    persistLayout(next: MerulaLayoutState) {
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

    // ── Named project workspaces (groups of `.merula` projects) ──────────────────
    /** Create a workspace (next palette colour by count), make it active, persist.
     *  Returns its id. */
    createWorkspace(name: string): string {
      const id = crypto.randomUUID();
      workspaces = [...workspaces, { id, name: name.trim() || 'Workspace', color_idx: workspaces.length, project_paths: [] }];
      activeWorkspace = id;
      persist();
      return id;
    },
    renameWorkspace(id: string, name: string) {
      const trimmed = name.trim();
      if (!trimmed) return;
      workspaces = workspaces.map(w => (w.id === id ? { ...w, name: trimmed } : w));
      persist();
    },
    setWorkspaceColor(id: string, colorIdx: number) {
      workspaces = workspaces.map(w => (w.id === id ? { ...w, color_idx: colorIdx } : w));
      persist();
    },
    deleteWorkspace(id: string) {
      workspaces = workspaces.filter(w => w.id !== id);
      if (activeWorkspace === id) activeWorkspace = null;
      persist();
    },
    /** Set (or clear) the active workspace. Persists. */
    setActiveWorkspace(id: string | null) {
      if (activeWorkspace === id) return;
      activeWorkspace = id;
      persist();
    },
    /** Add a project folder to a workspace (dedupe). Persists. */
    addProjectToWorkspace(id: string, path: string) {
      workspaces = workspaces.map(w =>
        w.id === id && !w.project_paths.includes(path)
          ? { ...w, project_paths: [...w.project_paths, path] }
          : w,
      );
      persist();
    },
    /** Remove a project folder from a workspace. Persists. */
    removeProjectFromWorkspace(id: string, path: string) {
      workspaces = workspaces.map(w =>
        w.id === id ? { ...w, project_paths: w.project_paths.filter(p => p !== path) } : w,
      );
      persist();
    },
  };
}

export const workspaceStore = createWorkspaceStore();
