/**
 * NemusShell UI state — the panel/layout/selection spine. The transport (run /
 * cycle), the log threshold and the diagnostics/log streams live in the engine +
 * config stores (`stores/engine.svelte`, `stores/config.svelte`); the open
 * project + its `.nemus` sources live in `stores/project.svelte`. This store owns
 * only the window-local UI: which side panels are open, per-track mute/solo, the
 * collapse/zen toggles, the Ctrl+F find relay, the overlay (settings / shortcuts
 * / command palette) toggles, the Outline→editor jump relay and the live caret
 * position. Follows Arbor's rune-store pattern (factory + getters).
 *
 * Layout (panel choices + collapse flags) is mirrored to the persisted nemus
 * window state via `layoutSnapshot()` / `applyLayout()` (NemusShell wires the
 * persistence + restore).
 */

import { LOG_LEVELS, type NemusLogThreshold } from './stores/config.svelte';
import { transportStore } from './stores/engine.svelte';
import type { NemusLayoutState } from '$lib/ipc/nemus';
import type { ControlEdit } from './editor/nemus-edit';

/** Left-rail panels (top group = side panels, bottom group = bottom panel). */
export type LeftPanel = 'files' | 'outline' | 'soundbank';
/** Bottom-docked panels. The Mixer lives here (Logic-style horizontal strips),
 *  alongside the Console + Problems + the background Jobs output, plus the
 *  instrument **Preview** (audition keyboard + knobs). (Find-usages is a floating
 *  popover, not a docked panel — see `usagesStore`.) */
export type BottomPanel = 'console' | 'problems' | 'mixer' | 'jobs' | 'preview';
/** Right-rail panels. */
export type RightPanel = 'inspector' | 'docs';

/** A one-shot request to jump the editor to a source offset (Outline click). */
export interface GotoRequest {
  /** UTF-16 offset of the symbol's name token. */
  offset: number;
  /** 1-based line (fallback when the offset can't be resolved). */
  line: number;
  /** Monotonic id so the same target fired twice still re-triggers. */
  seq: number;
}

/** A one-shot request to commit mixer/inspector knob values into the source as
 *  literals (the editor resolves spans + applies the edit — see nemus-edit). */
export interface CommitRequest {
  /** Track index (declaration order) whose pattern chain to edit. */
  index: number;
  /** The control literals to write (`gain`/`pan`/`room`/`delay`). */
  edits: ControlEdit[];
  /** Monotonic id so an identical commit still re-triggers. */
  seq: number;
}

function createNemusStore() {
  // ── Selection ──────────────────────────────────────────────────────────────
  // Track selection is keyed by the BE-stable strip INDEX (as a string), shared
  // with the mixer + inspector (`stores/mixer.svelte`). The file/source model
  // lives entirely in `stores/project.svelte` (path-keyed).
  let selectedTrackId = $state<string | null>(null);
  // Per-track mute/solo — single source of truth so the arrangement headers and
  // the mixer strips stay in sync (toggling one reflects in the other). Keyed by
  // `String(stripIndex)`; starts empty (the live arrangement seeds nothing).
  let muted  = $state<Record<string, boolean>>({});
  let soloed = $state<Record<string, boolean>>({});
  // Pre-mute gain snapshot (index key) — captured when a track is muted so unmute
  // can rewrite the source `.gain(x)` back (mute writes `.gain(0)` into the
  // `.nemus`; see mixer store `muteToSource`). Survives eval re-baseline because
  // the muted state itself does (the source carries the `.gain(0)` across evals).
  let premuteGain = $state<Record<string, number>>({});

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

  // ── Overlays (single mount in NemusShell; opened from menu + shortcuts) ──────
  let settingsOpen  = $state(false);
  let shortcutsOpen = $state(false);
  let paletteOpen   = $state(false);
  let renameProjectOpen = $state(false);
  let docsOpen      = $state(false);

  // ── Outline → editor jump relay (one-shot) ───────────────────────────────────
  let gotoRequest = $state<GotoRequest | null>(null);
  let gotoSeq = 0;

  // ── Mixer/Inspector → editor commit relay (one-shot) ─────────────────────────
  let commitRequest = $state<CommitRequest | null>(null);
  let commitSeq = 0;

  // ── Find-usages relay (one-shot) — the editor owns the tree + caret, so the
  // shortcut / palette ask it to collect usages via this bumped seq. ────────────
  let findUsagesSeq = $state(0);

  // ── Format-document relay (one-shot) — the editor owns the live buffer, so the
  // shortcut / palette ask it to reformat via this bumped seq. ──────────────────
  let formatSeq = $state(0);

  // ── Structure-popup relay (one-shot, Ctrl+F12) — the editor owns the tree, so
  // the shortcut / palette ask it to open the file-structure picker. ────────────
  let structureSeq = $state(0);

  // ── Refactor relays (one-shot) — rename / extract / inline are driven by the
  // editor (tree + selection); the palette asks for them via these bumped seqs. ─
  let renameSeq  = $state(0);
  let extractSeq = $state(0);
  let inlineSeq  = $state(0);

  // ── Intentions popup relay (one-shot, Alt+Enter) ──────────────────────────────
  let intentionsSeq = $state(0);

  // ── Live editor caret (footer Ln/Col) ────────────────────────────────────────
  let caretLine = $state(1);
  let caretCol  = $state(1);

  return {
    // per-track mute / solo
    isMuted(id: string)  { return !!muted[id]; },
    isSoloed(id: string) { return !!soloed[id]; },
    toggleMute(id: string)  { muted  = { ...muted,  [id]: !muted[id] }; },
    toggleSolo(id: string)  { soloed = { ...soloed, [id]: !soloed[id] }; },
    get anySolo() { return Object.values(soloed).some(Boolean); },

    // Pre-mute gain snapshot — set on mute, read + cleared on unmute so the source
    // `.gain` can be restored (mute writes `.gain(0)`). Owned here next to `muted`
    // so the mixer + arrangement context-menu share one mute/premute truth.
    premuteGain(id: string): number | undefined { return premuteGain[id]; },
    setPremuteGain(id: string, v: number) { premuteGain = { ...premuteGain, [id]: v }; },
    clearPremuteGain(id: string) {
      const { [id]: _drop, ...rest } = premuteGain;
      premuteGain = rest;
    },

    // Transport read-throughs (no local state — the engine stream owns these).
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
    /** Ensure a side panel is shown (used by shortcuts that focus a panel). */
    showLeft(p: LeftPanel)   { leftPanel = p; },
    showRight(p: RightPanel) { rightPanel = p; },
    showBottom(p: BottomPanel) { bottomPanel = p; },

    // layout
    get collapseUi()      { return collapseUi; },
    toggleCollapseUi()    { collapseUi = !collapseUi; if (collapseUi) collapseTabpane = false; },
    get collapseTabpane() { return collapseTabpane; },
    toggleCollapseTabpane() { collapseTabpane = !collapseTabpane; if (collapseTabpane) collapseUi = false; },
    get zen() { return zen; },
    toggleZen() { zen = !zen; },

    /** The five persisted layout fields, for mirroring to the workspace state. */
    layoutSnapshot(): NemusLayoutState {
      return {
        left_panel:      leftPanel,
        bottom_panel:    bottomPanel,
        right_panel:     rightPanel,
        collapse_viz:    collapseUi,
        collapse_editor: collapseTabpane,
      };
    },
    /** Restore the five layout fields from a loaded workspace state. */
    applyLayout(l: NemusLayoutState) {
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

    // ── overlays ──
    get settingsOpen()  { return settingsOpen; },
    openSettings()  { settingsOpen = true; },
    closeSettings() { settingsOpen = false; },
    get shortcutsOpen() { return shortcutsOpen; },
    openShortcuts()  { shortcutsOpen = true; },
    closeShortcuts() { shortcutsOpen = false; },
    get paletteOpen() { return paletteOpen; },
    openPalette()   { paletteOpen = true; },
    closePalette()  { paletteOpen = false; },
    togglePalette() { paletteOpen = !paletteOpen; },
    get renameProjectOpen() { return renameProjectOpen; },
    openRenameProject()  { renameProjectOpen = true; },
    closeRenameProject() { renameProjectOpen = false; },
    get docsOpen() { return docsOpen; },
    openDocs()   { docsOpen = true; },
    closeDocs()  { docsOpen = false; },
    toggleDocs() { docsOpen = !docsOpen; },

    // ── Outline / Problems → editor jump (one-shot; TabbedEditor consumes) ──
    get gotoRequest() { return gotoRequest; },
    requestGoto(offset: number, line: number) {
      // Jumping to source implies showing the editor — un-hide it if collapsed.
      if (collapseTabpane) collapseTabpane = false;
      gotoRequest = { offset, line, seq: ++gotoSeq };
    },

    // ── Mixer / Inspector → editor commit (one-shot; TabbedEditor consumes) ──
    // Writing a knob value into the source needs the editor's live tree, so the
    // request is relayed here and the editor resolves + applies it (one undo).
    get commitRequest() { return commitRequest; },
    requestCommit(index: number, edits: ControlEdit[]) {
      if (!edits.length) return;
      if (collapseTabpane) collapseTabpane = false; // editor must be mounted to apply
      commitRequest = { index, edits, seq: ++commitSeq };
    },

    // ── find usages (one-shot; TabbedEditor consumes, populates usagesStore) ──
    get findUsagesSeq() { return findUsagesSeq; },
    requestFindUsages() {
      if (collapseTabpane) collapseTabpane = false; // editor must be mounted to read the tree
      findUsagesSeq++;
    },

    // ── format document (one-shot; TabbedEditor consumes) ──
    get formatSeq() { return formatSeq; },
    requestFormat() {
      if (collapseTabpane) collapseTabpane = false; // editor must be mounted to reformat
      formatSeq++;
    },

    // ── structure popup / find method (one-shot; TabbedEditor consumes) ──
    get structureSeq() { return structureSeq; },
    requestStructure() {
      if (collapseTabpane) collapseTabpane = false; // editor must be mounted to read the tree
      structureSeq++;
    },

    // ── refactors (one-shot; TabbedEditor consumes) ──
    get renameSeq()  { return renameSeq; },
    get extractSeq() { return extractSeq; },
    get inlineSeq()  { return inlineSeq; },
    requestRename()  { if (collapseTabpane) collapseTabpane = false; renameSeq++; },
    requestExtract() { if (collapseTabpane) collapseTabpane = false; extractSeq++; },
    requestInline()  { if (collapseTabpane) collapseTabpane = false; inlineSeq++; },

    // ── intentions popup (one-shot; TabbedEditor consumes) ──
    get intentionsSeq() { return intentionsSeq; },
    requestIntentions() { if (collapseTabpane) collapseTabpane = false; intentionsSeq++; },

    // ── live caret (footer) ──
    get caretLine() { return caretLine; },
    get caretCol()  { return caretCol; },
    setCaret(line: number, col: number) { caretLine = line; caretCol = col; },
  };
}

export const nemusStore = createNemusStore();

/** Levels at or above a threshold (console emission gating). */
export function levelsAtOrAbove(threshold: string): Set<string> {
  const start = LOG_LEVELS.indexOf(threshold as NemusLogThreshold);
  return new Set<string>(LOG_LEVELS.slice(start < 0 ? 0 : start));
}

// Re-export so ConsolePanel / NemusTitleBar keep a single import site for the
// canonical level list (the source of truth is the config store).
export { LOG_LEVELS };
