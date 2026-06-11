/**
 * GroveShell UI state — the panel/layout/selection spine. The transport (run /
 * cycle), the log threshold and the diagnostics/log streams now live in the
 * engine + config stores (`stores/engine.svelte`, `stores/config.svelte`); this
 * store owns only the window-local UI: which side panels are open, the active
 * file/track selection, per-track mute/solo, the collapse/zen toggles and the
 * Ctrl+F find relay. Follows Arbor's rune-store pattern (factory + getters).
 *
 * Layout (panel choices + collapse flags) is mirrored to the persisted grove
 * window state via `layoutSnapshot()` / `applyLayout()` (GroveShell wires the
 * persistence + restore).
 */

import { LOG_LEVELS, type GroveLogThreshold } from './stores/config.svelte';
import { transportStore } from './stores/engine.svelte';
import type { GroveLayoutState } from '$lib/ipc/grove';
import { MOCK_PROJECT, MOCK_TRACKS } from './mock/data';

/** Left-rail panels (top group = side panels, bottom group = bottom panel). */
export type LeftPanel = 'files' | 'outline' | 'soundbank';
/** Bottom-docked panels. The Mixer lives here (Logic-style horizontal strips),
 *  alongside the Console + Problems. */
export type BottomPanel = 'console' | 'problems' | 'mixer';
/** Right-rail panels. */
export type RightPanel = 'inspector' | 'docs';

function createGroveStore() {
  // ── Selection ──────────────────────────────────────────────────────────────
  // The real file/source model lives in `stores/project.svelte` (path-keyed).
  // The Step-0 mock panels (Files / Outline / TabbedEditor) still drive off the
  // mock id-keyed selection below; the editor fan-out (Step 2/3) migrates them
  // onto the project store. Track selection likewise keys off mock track ids.
  let activeFileId  = $state<string>(MOCK_PROJECT.files[0].id);
  let openFileIds   = $state<string[]>([MOCK_PROJECT.files[0].id]);
  let selectedTrackId = $state<string | null>('t-bass');
  // Per-track mute/solo — single source of truth so the arrangement headers and
  // the mixer strips stay in sync (toggling in one reflects in the other).
  let muted  = $state<Record<string, boolean>>(Object.fromEntries(MOCK_TRACKS.map(t => [t.id, t.muted])));
  let soloed = $state<Record<string, boolean>>(Object.fromEntries(MOCK_TRACKS.map(t => [t.id, t.soloed])));

  // ── Rails / panels ─────────────────────────────────────────────────────────
  let leftPanel   = $state<LeftPanel | null>('files');
  let bottomPanel = $state<BottomPanel | null>('console');
  let rightPanel  = $state<RightPanel | null>('inspector');

  // ── Layout toggles ─────────────────────────────────────────────────────────
  let collapseUi      = $state(false);  // hide the viz pane (editor full width)
  let collapseTabpane = $state(false);  // hide the editor (viz full width)
  let zen             = $state(false);  // hide chrome (rails / footer / bottom)

  // ── Find (Ctrl+F) — set true to ask the active bottom panel to focus its
  // search input; the panel clears it once focused. */
  let findPending     = $state(false);

  return {
    // mock file selection (Step-0 panels) — see note above.
    get activeFileId() { return activeFileId; },
    setActiveFile(id: string) {
      activeFileId = id;
      if (!openFileIds.includes(id)) openFileIds = [...openFileIds, id];
    },
    get openFileIds() { return openFileIds; },
    openFile(id: string) {
      if (!openFileIds.includes(id)) openFileIds = [...openFileIds, id];
      activeFileId = id;
    },
    closeFile(id: string) {
      const idx = openFileIds.indexOf(id);
      if (idx === -1) return;
      openFileIds = openFileIds.filter(x => x !== id);
      if (activeFileId === id && openFileIds.length) {
        activeFileId = openFileIds[Math.min(idx, openFileIds.length - 1)];
      }
    },

    // per-track mute / solo
    isMuted(id: string)  { return !!muted[id]; },
    isSoloed(id: string) { return !!soloed[id]; },
    toggleMute(id: string)  { muted  = { ...muted,  [id]: !muted[id] }; },
    toggleSolo(id: string)  { soloed = { ...soloed, [id]: !soloed[id] }; },
    get anySolo() { return Object.values(soloed).some(Boolean); },

    // Transport read-throughs (no local state — the engine stream owns these).
    // Kept so the Step-0 viz panel (ArrangementView) compiles unchanged until
    // the fan-out migrates it onto `transportStore` directly.
    get running() { return transportStore.playing; },
    get cycle()   { return transportStore.cycle; },

    get selectedTrackId() { return selectedTrackId; },
    selectTrack(id: string | null) {
      selectedTrackId = id;
      // Selecting a track opens the Inspector only when the right rail is
      // otherwise empty — don't yank Docs away if the user has it open.
      if (id && rightPanel === null) rightPanel = 'inspector';
    },

    // rails
    get leftPanel()   { return leftPanel; },
    toggleLeft(p: LeftPanel)   { leftPanel   = leftPanel   === p ? null : p; },
    get bottomPanel() { return bottomPanel; },
    toggleBottom(p: BottomPanel) { bottomPanel = bottomPanel === p ? null : p; },
    get rightPanel()  { return rightPanel; },
    toggleRight(p: RightPanel)  { rightPanel  = rightPanel  === p ? null : p; },

    // layout
    get collapseUi()      { return collapseUi; },
    toggleCollapseUi()    { collapseUi = !collapseUi; if (collapseUi) collapseTabpane = false; },
    get collapseTabpane() { return collapseTabpane; },
    toggleCollapseTabpane() { collapseTabpane = !collapseTabpane; if (collapseTabpane) collapseUi = false; },
    get zen() { return zen; },
    toggleZen() { zen = !zen; },

    /** The five persisted layout fields, for mirroring to the workspace state. */
    layoutSnapshot(): GroveLayoutState {
      return {
        left_panel:      leftPanel,
        bottom_panel:    bottomPanel,
        right_panel:     rightPanel,
        collapse_viz:    collapseUi,
        collapse_editor: collapseTabpane,
      };
    },
    /** Restore the five layout fields from a loaded workspace state. */
    applyLayout(l: GroveLayoutState) {
      leftPanel       = (l.left_panel as LeftPanel | null) ?? null;
      bottomPanel     = (l.bottom_panel as BottomPanel | null) ?? null;
      rightPanel      = (l.right_panel as RightPanel | null) ?? null;
      collapseUi      = !!l.collapse_viz;
      collapseTabpane = !!l.collapse_editor;
    },

    // find (Ctrl+F): ensure a searchable bottom panel is shown, then ask it to
    // focus its search field.
    get findPending() { return findPending; },
    requestFind() {
      if (bottomPanel !== 'console' && bottomPanel !== 'problems') bottomPanel = 'console';
      findPending = true;
    },
    clearFind() { findPending = false; },
  };
}

export const groveStore = createGroveStore();

/** Levels at or above a threshold (console emission gating). */
export function levelsAtOrAbove(threshold: string): Set<string> {
  const start = LOG_LEVELS.indexOf(threshold as GroveLogThreshold);
  return new Set<string>(LOG_LEVELS.slice(start < 0 ? 0 : start));
}

// Re-export so ConsolePanel / GroveTitleBar keep a single import site for the
// canonical level list (the source of truth is the config store).
export { LOG_LEVELS };
