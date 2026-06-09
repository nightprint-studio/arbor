import { getExplorerConfig, setExplorerConfig } from '$lib/ipc/config';
import type { ExplorerConfig, ExplorerView, ExplorerSort, ExplorerStartup, ExplorerSectionConfig } from '$lib/types/config';

const DEFAULT: ExplorerConfig = {
  git_awareness:         false,
  global_shortcut:       false,
  default_view:          'details',
  show_hidden:           false,
  recursive_search:      false,
  global_shortcut_accel: 'Ctrl+Shift+E',
  default_sort:          'name',
  sort_ascending:        true,
  startup:               'overview',
  always_new_window:     false,
  max_recents:           10,
  sidebar_sections:      [],
};

export const MAX_RECENTS_MIN = 1;
export const MAX_RECENTS_MAX = 50;

/** Canonical sidebar sections in built-in order, with display labels. The
 *  config stores order + visibility against these ids. */
export const EXPLORER_SECTIONS: { id: string; label: string }[] = [
  { id: 'library',    label: 'Library' },
  { id: 'recents',    label: 'Recents' },
  { id: 'favourites', label: 'Favourites' },
  { id: 'devices',    label: 'Devices' },
  { id: 'projects',   label: 'Projects' },
];

/** Resolve the persisted (possibly empty/partial) section list against the
 *  canonical set: keep the saved order, drop unknown ids, append any sections
 *  missing from the saved list in their built-in position (shown by default). */
export function mergeSidebarSections(saved: ExplorerSectionConfig[]): { id: string; visible: boolean }[] {
  const known = new Set(EXPLORER_SECTIONS.map(s => s.id));
  const seen = new Set<string>();
  const out: { id: string; visible: boolean }[] = [];
  for (const s of saved ?? []) {
    if (known.has(s.id) && !seen.has(s.id)) { out.push({ id: s.id, visible: s.visible !== false }); seen.add(s.id); }
  }
  for (const s of EXPLORER_SECTIONS) {
    if (!seen.has(s.id)) out.push({ id: s.id, visible: true });
  }
  return out;
}

function clampRecents(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT.max_recents;
  return Math.max(MAX_RECENTS_MIN, Math.min(MAX_RECENTS_MAX, Math.round(n)));
}

const VIEWS: ExplorerView[] = ['details', 'medium', 'large', 'xlarge'];
const SORTS: ExplorerSort[] = ['name', 'modified', 'size'];
function normView(v: unknown): ExplorerView {
  return VIEWS.includes(v as ExplorerView) ? (v as ExplorerView) : 'details';
}
function normSort(v: unknown): ExplorerSort {
  return SORTS.includes(v as ExplorerSort) ? (v as ExplorerSort) : 'name';
}
function normStartup(v: unknown): ExplorerStartup {
  return v === 'last' ? 'last' : 'overview';
}

/**
 * Built-in File Explorer preferences, persisted in `~/.config/arbor/config.toml`
 * via the backend (never localStorage). Loaded from both AppShell and the
 * standalone ExplorerWindow (separate JS contexts → separate store instances).
 *
 * The two host-level switches (`gitAwareness`, `globalShortcut`) are edited
 * both from the SettingsPanel and the explorer's own settings page. Most
 * setters persist fire-and-forget; the global-shortcut setters are async and
 * rethrow so the UI can surface (and revert on) a registration conflict.
 */
function createExplorerStore() {
  let gitAwareness        = $state<boolean>(DEFAULT.git_awareness);
  let globalShortcut      = $state<boolean>(DEFAULT.global_shortcut);
  let defaultView         = $state<ExplorerView>(DEFAULT.default_view);
  let showHidden          = $state<boolean>(DEFAULT.show_hidden);
  let recursiveSearch     = $state<boolean>(DEFAULT.recursive_search);
  let globalShortcutAccel = $state<string>(DEFAULT.global_shortcut_accel);
  let defaultSort         = $state<ExplorerSort>(DEFAULT.default_sort);
  let sortAscending       = $state<boolean>(DEFAULT.sort_ascending);
  let startup             = $state<ExplorerStartup>(DEFAULT.startup);
  let alwaysNewWindow     = $state<boolean>(DEFAULT.always_new_window);
  let maxRecents          = $state<number>(DEFAULT.max_recents);
  let sidebarSections     = $state<ExplorerSectionConfig[]>([]);
  let loaded              = $state(false);

  async function loadConfig() {
    try {
      const cfg = await getExplorerConfig();
      gitAwareness        = !!cfg.git_awareness;
      globalShortcut      = !!cfg.global_shortcut;
      defaultView         = normView(cfg.default_view);
      showHidden          = !!cfg.show_hidden;
      recursiveSearch     = !!cfg.recursive_search;
      globalShortcutAccel = (cfg.global_shortcut_accel || DEFAULT.global_shortcut_accel).trim();
      defaultSort         = normSort(cfg.default_sort);
      sortAscending       = !!cfg.sort_ascending;
      startup             = normStartup(cfg.startup);
      alwaysNewWindow     = !!cfg.always_new_window;
      maxRecents          = clampRecents(cfg.max_recents);
      sidebarSections     = Array.isArray(cfg.sidebar_sections) ? cfg.sidebar_sections : [];
      loaded = true;
    } catch {
      // First-run / backend not ready — keep defaults; next call retries.
    }
  }

  function snapshot(): ExplorerConfig {
    return {
      git_awareness:         gitAwareness,
      global_shortcut:       globalShortcut,
      default_view:          defaultView,
      show_hidden:           showHidden,
      recursive_search:      recursiveSearch,
      global_shortcut_accel: globalShortcutAccel,
      default_sort:          defaultSort,
      sort_ascending:        sortAscending,
      startup,
      always_new_window:     alwaysNewWindow,
      max_recents:           maxRecents,
      sidebar_sections:      sidebarSections,
    };
  }

  function persist() { void setExplorerConfig(snapshot()).catch(() => {}); }
  /** Persist and rethrow — used by the global-shortcut path so the caller can
   *  show a toast and revert when the backend can't register the combo. */
  function persistThrow() { return setExplorerConfig(snapshot()); }

  function setGitAwareness(on: boolean)    { if (gitAwareness === on) return;    gitAwareness = on;    persist(); }
  function setDefaultView(v: ExplorerView) { const n = normView(v); if (defaultView === n) return; defaultView = n; persist(); }
  function setShowHidden(on: boolean)      { if (showHidden === on) return;      showHidden = on;      persist(); }
  function setRecursiveSearch(on: boolean) { if (recursiveSearch === on) return; recursiveSearch = on; persist(); }
  function setDefaultSort(v: ExplorerSort) { const n = normSort(v); if (defaultSort === n) return; defaultSort = n; persist(); }
  function setSortAscending(on: boolean)   { if (sortAscending === on) return;   sortAscending = on;   persist(); }
  function setStartup(v: ExplorerStartup)  { const n = normStartup(v); if (startup === n) return; startup = n; persist(); }
  function setAlwaysNewWindow(on: boolean) { if (alwaysNewWindow === on) return; alwaysNewWindow = on; persist(); }
  function setMaxRecents(n: number)        { const c = clampRecents(n); if (maxRecents === c) return; maxRecents = c; persist(); }
  /** Replace the sidebar section order/visibility (already a resolved list). */
  function setSidebarSections(list: ExplorerSectionConfig[]) { sidebarSections = list; persist(); }

  /** Enable/disable the global shortcut. Reverts on a registration conflict. */
  async function setGlobalShortcut(on: boolean) {
    if (globalShortcut === on) return;
    const prev = globalShortcut;
    globalShortcut = on;
    try { await persistThrow(); }
    catch (e) { globalShortcut = prev; throw e; }
  }
  /** Rebind the global shortcut accelerator. Reverts on conflict/invalid combo. */
  async function setGlobalShortcutAccel(accel: string) {
    const next = accel.trim();
    if (!next || globalShortcutAccel === next) return;
    const prev = globalShortcutAccel;
    globalShortcutAccel = next;
    try { await persistThrow(); }
    catch (e) { globalShortcutAccel = prev; throw e; }
  }

  return {
    get gitAwareness()        { return gitAwareness; },
    get globalShortcut()      { return globalShortcut; },
    get defaultView()         { return defaultView; },
    get showHidden()          { return showHidden; },
    get recursiveSearch()     { return recursiveSearch; },
    get globalShortcutAccel() { return globalShortcutAccel; },
    get defaultSort()         { return defaultSort; },
    get sortAscending()       { return sortAscending; },
    get startup()             { return startup; },
    get alwaysNewWindow()     { return alwaysNewWindow; },
    get maxRecents()          { return maxRecents; },
    get sidebarSections()     { return sidebarSections; },
    get loaded()              { return loaded; },
    loadConfig,
    setGitAwareness,
    setDefaultView,
    setShowHidden,
    setRecursiveSearch,
    setDefaultSort,
    setSortAscending,
    setStartup,
    setAlwaysNewWindow,
    setMaxRecents,
    setSidebarSections,
    setGlobalShortcut,
    setGlobalShortcutAccel,
  };
}

export const explorerStore = createExplorerStore();
