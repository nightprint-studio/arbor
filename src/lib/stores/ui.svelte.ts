import { getRecentRepos, addRecentRepo as addRecentRepoIpc } from '$lib/ipc/config';
import type { StashEntry } from '$lib/types/git';

export type Panel = 'graph' | 'settings' | 'plugins' | 'rebase' | 'about' | 'docs';
export type ToastKind = 'info' | 'success' | 'warning' | 'error';
/**
 * Bottom panel slot. Historically an enum of built-in sections; now also accepts
 * plugin-registered bottom panels encoded as `"plugin:<name>:<id>"`. Only ONE
 * bottom panel is ever visible at a time — clicking any bottom-area icon
 * overrides whichever panel was open, regardless of which ActivityBar (left or
 * right) fired the click.
 */
export type BuiltinBottomSection = 'stage' | 'detail' | 'terminal' | 'jobs' | 'pipelines' | 'plugin-logs';
export type BottomSection = BuiltinBottomSection | `plugin:${string}`;

export interface ToastAction {
  label: string;
  /** Side-effect to run when the user clicks the action button. The toast
   *  is dismissed automatically afterwards. Kept as a closure (not data)
   *  because toasts don't survive a reload — for persisted click actions
   *  use `notificationsStore.add(..., action)` instead. */
  onClick: () => void;
}

export interface Toast {
  id: string;
  kind: ToastKind;
  message: string;
  duration: number;
  /** Wall-clock ms when the toast was added.  Used by the unified
   *  bottom-right stack to interleave toasts with notifications in
   *  chronological order. */
  addedAt: number;
  /** Optional clickable action rendered as a button on the right side
   *  of the toast (e.g. "Open" → deep-links to a pipeline run). */
  action?: ToastAction;
}

const SIDEBAR_RATIO_KEY       = 'arbor:sidebar-ratio';
const RIGHT_SIDEBAR_RATIO_KEY = 'arbor:right-sidebar-ratio';
const BOTTOM_RATIO_KEY        = 'arbor:bottom-ratio';
const SIDEBAR_SECTION_KEY     = 'arbor:sidebar-section';
// Mirror of *_LAST_SECTION_KEY for the left rail — lets the generic Ctrl+B
// "toggle sidebar visibility" shortcut re-open whichever section was last
// active after the user collapsed the rail. SIDEBAR_SECTION_KEY itself is
// cleared on close, so a separate slot is needed.
const SIDEBAR_LAST_SECTION_KEY = 'arbor:sidebar-last-section';
const RIGHT_SIDEBAR_SECTION_KEY      = 'arbor:right-sidebar-section';
// Tracks the last right-sidebar section the user actually had open, so the
// "toggle expand/collapse" shortcut can re-open it when the rail is hidden.
// Distinct from RIGHT_SIDEBAR_SECTION_KEY because that one gets cleared when
// the user closes the rail.
const RIGHT_SIDEBAR_LAST_SECTION_KEY = 'arbor:right-sidebar-last-section';
const BOTTOM_SECTION_KEY      = 'arbor:bottom-section';
// Mirrors RIGHT_SIDEBAR_LAST_SECTION_KEY: tracks the last bottom section the
// user actually had open, so the "toggle visibility" shortcut can re-open it
// after the panel was closed (BOTTOM_SECTION_KEY gets cleared on close).
const BOTTOM_LAST_SECTION_KEY = 'arbor:bottom-last-section';

function loadPixels(key: string, defaultPx: number, min: number, max: number, useHeight = false): number {
  try {
    const ratio = parseFloat(localStorage.getItem(key) ?? '');
    if (!isNaN(ratio) && ratio > 0) {
      const ref = useHeight ? window.innerHeight : window.innerWidth;
      return Math.max(min, Math.min(max, Math.round(ratio * ref)));
    }
  } catch { /* ignore */ }
  return defaultPx;
}

function saveRatio(key: string, px: number, useHeight = false) {
  try {
    const ref = useHeight ? window.innerHeight : window.innerWidth;
    localStorage.setItem(key, String(px / ref));
  } catch { /* ignore */ }
}

function createUiStore() {
  let sidebarWidth      = $state(loadPixels(SIDEBAR_RATIO_KEY, 240, 160, 500));
  let rightSidebarWidth = $state(loadPixels(RIGHT_SIDEBAR_RATIO_KEY, 280, 160, 500));
  let bottomHeight      = $state(loadPixels(BOTTOM_RATIO_KEY, 280, 100, 600, true));

  let activePanel       = $state<Panel>('graph');
  let activeSidebarSection = $state<string | null>(
    (() => { try { return localStorage.getItem(SIDEBAR_SECTION_KEY) ?? 'branches'; } catch { return 'branches'; } })()
  );
  // Right sidebar starts collapsed — nothing lives there by default. Restored
  // to the last-selected right section on app start if the user had one open.
  let activeRightSidebar = $state<string | null>(
    (() => { try { return localStorage.getItem(RIGHT_SIDEBAR_SECTION_KEY); } catch { return null; } })()
  );
  let activeBottomSection = $state<BottomSection | null>(null);

  // ── Bottom-panel readiness signal ────────────────────────────────────────
  // Push-based notification used by the deep-link dispatcher (and any other
  // flow that needs to coordinate with the panel's mount + transition
  // lifecycle).  AppShell calls `notifyBottomPanelReady()` from:
  //   * the panel's `onintroend` (slide-in completed)
  //   * a `$effect` watching `activeBottomSection` that fires when the swap
  //     happens with no transition queued (panel was already mounted)
  // Consumers `await awaitBottomPanelReady()` BEFORE setActiveBottomSection
  // so they don't miss the signal — the helper times out at 500ms as a
  // safety net.
  const bottomPanelReadyWaiters: Array<() => void> = [];

  function notifyBottomPanelReady() {
    if (bottomPanelReadyWaiters.length === 0) return;
    const queue = bottomPanelReadyWaiters.splice(0);
    for (const fn of queue) fn();
  }

  function awaitBottomPanelReady(timeoutMs = 500): Promise<void> {
    return new Promise<void>(resolve => {
      let done = false;
      const finish = () => {
        if (done) return;
        done = true;
        const idx = bottomPanelReadyWaiters.indexOf(finish);
        if (idx >= 0) bottomPanelReadyWaiters.splice(idx, 1);
        resolve();
      };
      bottomPanelReadyWaiters.push(finish);
      window.setTimeout(finish, timeoutMs);
    });
  }

  let searchVisible            = $state(false);
  let commandPaletteOpen       = $state(false);
  /** Verb id the CommandPalette should auto-select on mount, cleared after
   *  the palette consumes it.  Drives the Ctrl+N / Ctrl+Shift+N shortcuts
   *  that pre-fill the palette with a workspace-aware verb. */
  let pendingPaletteVerb       = $state<string | null>(null);
  let jobsOverlayOpen          = $state(false);
  let notificationsOverlayOpen = $state(false);
  let securityOverlayOpen      = $state(false);
  let recentQuickSwitchOpen    = $state(false);
  let repoBrowserOpen          = $state(false);
  let mergeModalOpen                = $state(false);
  let stashConflictModalOpen        = $state(false);
  let checkoutConflictModalOpen     = $state(false);
  let checkoutConflictTabId         = $state<string | null>(null);
  let checkoutConflictBranch        = $state<string | null>(null);
  let stashConflictEntry       = $state<StashEntry | null>(null);
  let stashConflictFiles       = $state<string[]>([]);
  let stashBlockingFiles       = $state<string[]>([]);
  let stashBlockingPop         = $state(false);
  let toasts            = $state<Toast[]>([]);
  let isModalOpen       = $state(false);
  let modalContent      = $state<string | null>(null);
  let recentRepos       = $state<string[]>([]);
  let linkManagerOpen         = $state(false);
  /** Pre-selects a link when WorktreeLinkManagerModal opens (e.g. from a sync notification). */
  let linkManagerInitialId    = $state<string | null>(null);
  let addToLinkModalOpen      = $state(false);
  /** RepoRegistry UUID of the repo offered for "add to link" — drives AddToWorktreeLinkModal. */
  let addToLinkRepoId         = $state<string | null>(null);

  let appFocused        = $state(true);   // tracks window focus / visibility

  let toastCounter = 0;

  function setPanel(p: Panel) { activePanel = p; }
  function setAppFocused(v: boolean) { appFocused = v; }

  function setSidebarWidth(w: number) {
    sidebarWidth = Math.max(160, Math.min(500, w));
    saveRatio(SIDEBAR_RATIO_KEY, sidebarWidth);
  }
  function setRightSidebarWidth(w: number) {
    rightSidebarWidth = Math.max(160, Math.min(500, w));
    saveRatio(RIGHT_SIDEBAR_RATIO_KEY, rightSidebarWidth);
  }
  function setBottomHeight(h: number) {
    bottomHeight = Math.max(100, Math.min(600, h));
    saveRatio(BOTTOM_RATIO_KEY, bottomHeight, true);
  }

  function setActiveSidebarSection(section: string | null) {
    activeSidebarSection = section;
    try {
      if (section) {
        localStorage.setItem(SIDEBAR_SECTION_KEY, section);
        // Remember the last section the user actually had open so
        // toggleSidebarVisibility() can restore it after a collapse.
        localStorage.setItem(SIDEBAR_LAST_SECTION_KEY, section);
      } else {
        localStorage.removeItem(SIDEBAR_SECTION_KEY);
      }
    } catch { /* ignore */ }
  }

  function toggleSidebarSection(section: string) {
    setActiveSidebarSection(activeSidebarSection === section ? null : section);
  }

  /**
   * Generic left-rail expand/collapse — mirrors `toggleRightSidebarVisibility`
   * and `toggleBottomVisibility`. If a section is active, close the rail; if
   * collapsed, restore the last-used section (defaults to 'branches' on a
   * fresh install). This is the IDE-standard Ctrl+B behavior — the explicit
   * "open Branches" shortcut goes through `toggleSidebarSection('branches')`.
   */
  function toggleSidebarVisibility(): void {
    if (activeSidebarSection !== null) {
      setActiveSidebarSection(null);
      return;
    }
    let last: string | null = null;
    try { last = localStorage.getItem(SIDEBAR_LAST_SECTION_KEY); } catch { /* ignore */ }
    setActiveSidebarSection(last ?? 'branches');
  }

  /**
   * Toggle a section only if its ActivityBar button is currently visible —
   * matches IntelliJ's tool-window shortcut behavior (Alt+1..9 do nothing
   * when the corresponding tool window has been removed from the bar).
   * Imported lazily to avoid a top-level circular dep with the config store.
   */
  async function toggleSidebarSectionIfVisible(section: string): Promise<void> {
    const { activityBarConfigStore } = await import('./activityBarConfig.svelte');
    if (!activityBarConfigStore.isVisible(section)) return;
    toggleSidebarSection(section);
  }

  async function toggleBottomSectionIfVisible(section: BottomSection): Promise<void> {
    const { activityBarConfigStore } = await import('./activityBarConfig.svelte');
    if (!activityBarConfigStore.isVisible(section)) return;
    toggleBottomSection(section);
  }

  function setActiveRightSidebar(section: string | null) {
    activeRightSidebar = section;
    try {
      if (section) {
        localStorage.setItem(RIGHT_SIDEBAR_SECTION_KEY, section);
        // Remember which section was last open so toggleRightSidebarVisibility
        // can restore it after the user collapsed the rail.
        localStorage.setItem(RIGHT_SIDEBAR_LAST_SECTION_KEY, section);
      } else {
        localStorage.removeItem(RIGHT_SIDEBAR_SECTION_KEY);
      }
    } catch { /* ignore */ }
  }

  function toggleRightSidebar(section: string) {
    setActiveRightSidebar(activeRightSidebar === section ? null : section);
  }

  /** Expand/collapse the right rail without picking a specific section.
   *  - If a section is already active, collapse the rail.
   *  - If collapsed, restore the last section the user had open.  Falls back
   *    to the first registered right-side plugin section if there's no
   *    history yet (e.g. fresh install).  Returns false silently when the
   *    user has no right-side plugin sections at all (the rail isn't
   *    rendered then, so there's nothing to expand). */
  function toggleRightSidebarVisibility(): void {
    if (activeRightSidebar !== null) {
      setActiveRightSidebar(null);
      return;
    }
    let last: string | null = null;
    try { last = localStorage.getItem(RIGHT_SIDEBAR_LAST_SECTION_KEY); } catch { /* ignore */ }
    if (last) {
      setActiveRightSidebar(last);
      return;
    }
    // No history — pick the first available right-side plugin section if
    // any.  Imported lazily to avoid a top-level circular dep with the
    // contribution store.
    Promise.all([
      import('./contribution.svelte'),
      import('$lib/contributions/sidebar'),
    ]).then(([{ contributionStore }, { SIDEBAR_POINT, parseSidebarSection }]) => {
      const first = contributionStore.forPoint(SIDEBAR_POINT)
        .map(parseSidebarSection)
        .find(s => s.side === 'right');
      if (first) setActiveRightSidebar(`plugin:${first.plugin_name}:${first.id}`);
    }).catch(() => { /* ignore */ });
  }

  /**
   * Close any sidebar / right-sidebar / bottom section that belongs to a
   * plugin. Pass a name to scope to that plugin, or omit it to close every
   * plugin-owned section (used when the master kill-switch flips off).
   *
   * Section keys are formatted `plugin:<name>:<id>` for sidebars and
   * `plugin:<…>` for the bottom section, so a single prefix check is enough.
   */
  function closePluginSections(name?: string) {
    const matches = (key: string | null): boolean => {
      if (!key) return false;
      if (!name) return key.startsWith('plugin:');
      return key === `plugin:${name}` || key.startsWith(`plugin:${name}:`);
    };
    if (matches(activeSidebarSection))           setActiveSidebarSection(null);
    if (matches(activeRightSidebar))             setActiveRightSidebar(null);
    if (matches(activeBottomSection as string))  setActiveBottomSection(null);
  }

  // The section that was active immediately before the current one.
  // Drives "back" navigation in panels that opened on top of another panel
  // (currently: JobOutputPanel uses this to return to a plugin monitor like
  // run-monitor instead of always falling back to the global JobsOverlay).
  //
  // We track ONLY direct A→B transitions where both A and B are non-null.
  // Closing the panel (B = null) resets `previousBottomSection` to null too,
  // otherwise a stale value lingers across "close → reopen elsewhere"
  // sequences (e.g. close run-monitor → open the JobsOverlay → click a job
  // would wrongly route Back into run-monitor).
  let previousBottomSection = $state<BottomSection | null>(null);

  function setActiveBottomSection(section: BottomSection | null) {
    if (section === null) {
      previousBottomSection = null;
    } else if (section !== activeBottomSection && activeBottomSection !== null) {
      previousBottomSection = activeBottomSection;
    }
    activeBottomSection = section;
    try {
      if (section) {
        localStorage.setItem(BOTTOM_SECTION_KEY, section);
        localStorage.setItem(BOTTOM_LAST_SECTION_KEY, section);
      } else {
        localStorage.removeItem(BOTTOM_SECTION_KEY);
      }
    } catch { /* ignore */ }
  }

  function toggleBottomSection(section: BottomSection) {
    setActiveBottomSection(activeBottomSection === section ? null : section);
  }

  /** Expand/collapse the bottom panel without picking a specific section.
   *  Mirrors `toggleRightSidebarVisibility`:
   *  - If a section is active, close the panel.
   *  - If collapsed, restore the last section the user had open. Falls back
   *    to 'stage' on a fresh install. */
  function toggleBottomVisibility(): void {
    if (activeBottomSection !== null) {
      setActiveBottomSection(null);
      return;
    }
    let last: string | null = null;
    try { last = localStorage.getItem(BOTTOM_LAST_SECTION_KEY); } catch { /* ignore */ }
    setActiveBottomSection((last as BottomSection | null) ?? 'stage');
  }

  function setSearchVisible(v: boolean)           { searchVisible = v; }
  function setCommandPaletteOpen(v: boolean)      { commandPaletteOpen = v; if (!v) pendingPaletteVerb = null; }
  function toggleCommandPalette()                 { commandPaletteOpen = !commandPaletteOpen; if (!commandPaletteOpen) pendingPaletteVerb = null; }
  function openCommandPaletteWithVerb(verbId: string) {
    pendingPaletteVerb = verbId;
    commandPaletteOpen = true;
  }
  function takePendingPaletteVerb(): string | null {
    const v = pendingPaletteVerb;
    pendingPaletteVerb = null;
    return v;
  }
  function toggleJobsOverlay()                    { jobsOverlayOpen = !jobsOverlayOpen; }
  function openMergeModal()                       { mergeModalOpen = true; }
  function closeMergeModal()                      { mergeModalOpen = false; }
  function openStashConflictModal(stash: StashEntry, files: string[] = [], blocking: string[] = [], isPop = false) {
    stashConflictEntry    = stash;
    stashConflictFiles    = files;
    stashBlockingFiles    = blocking;
    stashBlockingPop      = isPop;
    stashConflictModalOpen = true;
  }
  function closeStashConflictModal() {
    stashConflictModalOpen = false;
    stashConflictEntry     = null;
    stashConflictFiles     = [];
    stashBlockingFiles     = [];
    stashBlockingPop       = false;
  }
  function openCheckoutConflictModal(tabId: string, branch: string) {
    checkoutConflictTabId    = tabId;
    checkoutConflictBranch   = branch;
    checkoutConflictModalOpen = true;
  }
  function closeCheckoutConflictModal() {
    checkoutConflictModalOpen = false;
    checkoutConflictTabId     = null;
    checkoutConflictBranch    = null;
  }
  function toggleRecentQuickSwitch()              { recentQuickSwitchOpen = !recentQuickSwitchOpen; }
  function setRecentQuickSwitchOpen(v: boolean)   { recentQuickSwitchOpen = v; }
  function setJobsOverlayOpen(v: boolean)         { jobsOverlayOpen = v; }
  function toggleNotificationsOverlay()           { notificationsOverlayOpen = !notificationsOverlayOpen; }
  function setNotificationsOverlayOpen(v: boolean){ notificationsOverlayOpen = v; }
  function toggleSecurityOverlay()                { securityOverlayOpen = !securityOverlayOpen; }
  function setSecurityOverlayOpen(v: boolean)     { securityOverlayOpen = v; }
  function openRepoBrowser()                      { repoBrowserOpen = true; }
  function closeRepoBrowser()                     { repoBrowserOpen = false; }

  /** Load recent repos from backend. Runs once at startup.
   *  Also performs a one-time migration from localStorage if legacy data exists. */
  async function loadRecentRepos() {
    // One-time migration: if localStorage still has the old key, push each path
    // to the backend (reversed so the most-recent entry ends up first), then clear it.
    try {
      const legacy = JSON.parse(localStorage.getItem('arbor:recent_repos') ?? 'null');
      if (Array.isArray(legacy) && legacy.length > 0) {
        for (const p of [...legacy].reverse()) {
          await addRecentRepoIpc(p).catch(() => {});
        }
        localStorage.removeItem('arbor:recent_repos');
      }
    } catch { /* ignore */ }

    recentRepos = await getRecentRepos().catch(() => []);
  }

  function addRecentRepo(path: string) {
    const normalized = path.replace(/\\/g, '/');
    // Optimistic local update so the UI reflects the change immediately.
    recentRepos = [normalized, ...recentRepos.filter(p => p.replace(/\\/g, '/') !== normalized)].slice(0, 10);
    // Persist to backend (fire-and-forget).
    addRecentRepoIpc(normalized).catch(() => {});
  }

  function showToast(
    message: string,
    kind: ToastKind = 'info',
    duration = 3500,
    action?: ToastAction,
  ): string {
    const id = `toast-${++toastCounter}`;
    toasts.push({ id, kind, message, duration, addedAt: Date.now(), action });
    setTimeout(() => dismissToast(id), duration);
    return id;
  }

  function dismissToast(id: string) {
    const idx = toasts.findIndex(t => t.id === id);
    if (idx !== -1) toasts.splice(idx, 1);
  }

  function openModal(content: string)  { modalContent = content; isModalOpen = true; }
  function closeModal()                { isModalOpen = false; modalContent = null; }

  function openLinkManager(linkId: string | null = null) {
    linkManagerInitialId = linkId;
    linkManagerOpen      = true;
  }
  function closeLinkManager() {
    linkManagerOpen      = false;
    linkManagerInitialId = null;
  }
  function openAddToLink(repoId: string) {
    addToLinkRepoId    = repoId;
    addToLinkModalOpen = true;
  }
  function closeAddToLink() {
    addToLinkModalOpen = false;
    addToLinkRepoId    = null;
  }

  return {
    get sidebarWidth()           { return sidebarWidth; },
    get rightSidebarWidth()      { return rightSidebarWidth; },
    get bottomHeight()           { return bottomHeight; },
    get activePanel()            { return activePanel; },
    get activeSidebarSection()   { return activeSidebarSection; },
    get activeRightSidebar()     { return activeRightSidebar; },
    get activeBottomSection()    { return activeBottomSection; },
    get previousBottomSection()  { return previousBottomSection; },
    /** True when no sidebar section is active (sidebar is hidden). */
    get isSidebarCollapsed()     { return activeSidebarSection === null; },
    /** True when the right sidebar panel is collapsed (no icon active). */
    get isRightSidebarCollapsed(){ return activeRightSidebar === null; },
    get searchVisible()                  { return searchVisible; },
    get commandPaletteOpen()             { return commandPaletteOpen; },
    get pendingPaletteVerb()             { return pendingPaletteVerb; },
    get jobsOverlayOpen()                { return jobsOverlayOpen; },
    get notificationsOverlayOpen()       { return notificationsOverlayOpen; },
    get securityOverlayOpen()            { return securityOverlayOpen; },
    get recentQuickSwitchOpen()          { return recentQuickSwitchOpen; },
    get repoBrowserOpen()                { return repoBrowserOpen; },
    get mergeModalOpen()                 { return mergeModalOpen; },
    get stashConflictModalOpen()         { return stashConflictModalOpen; },
    get stashConflictEntry()             { return stashConflictEntry; },
    get stashConflictFiles()             { return stashConflictFiles; },
    get stashBlockingFiles()             { return stashBlockingFiles; },
    get stashBlockingPop()               { return stashBlockingPop; },
    get checkoutConflictModalOpen()      { return checkoutConflictModalOpen; },
    get checkoutConflictTabId()          { return checkoutConflictTabId; },
    get checkoutConflictBranch()         { return checkoutConflictBranch; },
    get toasts()                 { return toasts; },
    get isModalOpen()            { return isModalOpen; },
    get modalContent()           { return modalContent; },
    get recentRepos()            { return recentRepos; },
    get appFocused()             { return appFocused; },
    get linkManagerOpen()         { return linkManagerOpen; },
    get linkManagerInitialId()    { return linkManagerInitialId; },
    get addToLinkModalOpen()      { return addToLinkModalOpen; },
    get addToLinkRepoId()         { return addToLinkRepoId; },
    openLinkManager, closeLinkManager,
    openAddToLink, closeAddToLink,
    setPanel, setSidebarWidth, setRightSidebarWidth, setBottomHeight,
    setAppFocused,
    setActiveSidebarSection, toggleSidebarSection, toggleSidebarVisibility,
    toggleSidebarSectionIfVisible, toggleBottomSectionIfVisible,
    setActiveRightSidebar, toggleRightSidebar, toggleRightSidebarVisibility,
    setActiveBottomSection, toggleBottomSection, toggleBottomVisibility,
    notifyBottomPanelReady, awaitBottomPanelReady,
    closePluginSections,
    setSearchVisible, setCommandPaletteOpen, toggleCommandPalette,
    openCommandPaletteWithVerb, takePendingPaletteVerb,
    toggleJobsOverlay, setJobsOverlayOpen,
    openMergeModal, closeMergeModal,
    openStashConflictModal, closeStashConflictModal,
    openCheckoutConflictModal, closeCheckoutConflictModal,
    toggleNotificationsOverlay, setNotificationsOverlayOpen,
    toggleSecurityOverlay, setSecurityOverlayOpen,
    toggleRecentQuickSwitch, setRecentQuickSwitchOpen,
    openRepoBrowser, closeRepoBrowser,
    loadRecentRepos, addRecentRepo, showToast, dismissToast, openModal, closeModal,
  };
}

export const uiStore = createUiStore();
