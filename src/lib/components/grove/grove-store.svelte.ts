/**
 * GroveShell UI state (Step 0 — all mocked). Holds which side panels are open,
 * the transport play/stop state, the active file/track selection, the log
 * threshold and the layout toggles (collapse ui / collapse tabpane / zen).
 * Follows Arbor's canonical rune-store pattern (function factory + getters).
 *
 * Nothing here talks to a backend — `running` just flips a flag the footer and
 * Run/Stop button reflect. When the engine lands, the transport actions call
 * IPC; the surface stays identical.
 */

import type { LogLevel } from './mock/types';
import { MOCK_PROJECT, MOCK_TRACKS } from './mock/data';

/** Left-rail panels (top group = side panels, bottom group = bottom panel). */
export type LeftPanel = 'files' | 'outline' | 'soundbank';
/** Bottom-docked panels. The Mixer lives here (Logic-style horizontal strips),
 *  alongside the Console + Problems. */
export type BottomPanel = 'console' | 'problems' | 'mixer';
/** Right-rail panels. */
export type RightPanel = 'inspector' | 'docs';

const LOG_LEVELS: LogLevel[] = ['trace', 'debug', 'info', 'warn', 'error'];

function createGroveStore() {
  // ── Selection ──────────────────────────────────────────────────────────────
  let activeFileId  = $state<string>(MOCK_PROJECT.files[0].id);
  /** Files open as editor tabs (order = tab order). Seeded with song.grove. */
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

  // ── Transport / engine (mocked) ────────────────────────────────────────────
  let running   = $state(false);
  let cycle     = $state(0);        // absolute cycle position
  let logLevel  = $state<LogLevel>('info');

  // ── Layout toggles ─────────────────────────────────────────────────────────
  let collapseUi      = $state(false);  // hide the viz pane (editor full width)
  let collapseTabpane = $state(false);  // hide the editor (viz full width)
  let zen             = $state(false);  // hide chrome (rails / footer / bottom)

  // ── Find (Ctrl+F) — set true to ask the active bottom panel to focus its
  // search input; the panel clears it once focused. */
  let findPending     = $state(false);

  return {
    // selection
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

    // transport
    get running() { return running; },
    // Stopping never resets the clock — the cycle position is preserved, as in
    // the real engine (re-eval/stop keep continuity).
    toggleRun()   { running = !running; },
    get cycle()   { return cycle; },
    setCycle(c: number) { cycle = c; },
    get logLevel() { return logLevel; },
    setLogLevel(l: LogLevel) { logLevel = l; },

    // layout
    get collapseUi()      { return collapseUi; },
    toggleCollapseUi()    { collapseUi = !collapseUi; if (collapseUi) collapseTabpane = false; },
    get collapseTabpane() { return collapseTabpane; },
    toggleCollapseTabpane() { collapseTabpane = !collapseTabpane; if (collapseTabpane) collapseUi = false; },
    get zen() { return zen; },
    toggleZen() { zen = !zen; },

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

/** Levels at or above the current threshold (for the mock console gating). */
export function levelsAtOrAbove(threshold: LogLevel): Set<LogLevel> {
  const start = LOG_LEVELS.indexOf(threshold);
  return new Set(LOG_LEVELS.slice(start));
}

export { LOG_LEVELS };
