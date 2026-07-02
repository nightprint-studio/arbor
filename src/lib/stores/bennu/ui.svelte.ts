/**
 * Bennu window UI store — chrome state for the standalone Java-editor window:
 * which left / right / bottom dockable panel is open, the tree expansion set for
 * the Project tool, and the docs/settings/palette/find overlay flags. Pure
 * session UI state (no persistence needed yet) — mirrors the merula window's
 * `merula-store` shape at a smaller scale.
 *
 * Tool-window layout (IntelliJ New UI):
 *   • LEFT rail (top)     — Project (tree), Structure (symbols).
 *   • LEFT rail (bottom)  — the bottom-dock toggles: Terminal, Problems.
 *   • RIGHT rail          — Maven (top), Services/Run (bottom). Mock panels.
 *   • BOTTOM dock         — Problems + Terminal, tabbed. Its toggles live in the
 *                           left rail's bottom cluster.
 * Find-in-project is a modal (Ctrl+Shift+F), not a rail tool.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { SvelteSet } from 'svelte/reactivity';

/** Left tool windows (activity bar, top group). */
export type LeftPanel = 'project' | 'structure';
/** Right tool windows (activity bar) — mock tool panels for now. */
export type RightPanel = 'maven' | 'services';
/** Bottom dock sections (tabbed). */
export type BottomPanel = 'problems' | 'terminal';

function createBennuUiStore() {
  // Default the Project tool open so the shell shows the tree on launch.
  let leftPanel = $state<LeftPanel | null>('project');
  let rightPanel = $state<RightPanel | null>(null);
  let bottomPanel = $state<BottomPanel | null>(null);

  let settingsOpen = $state(false);
  let docsOpen = $state(false);
  let paletteOpen = $state(false);
  // Find-in-project modal (Ctrl+Shift+F).
  let findOpen = $state(false);

  // Project-tree expansion set (controlled Tree expansion) so the toolbar can
  // Collapse-all / Expand-all and Select-opened-file can reveal a path.
  const treeExpanded = new SvelteSet<string>();

  // Goto relay — a panel (Structure / Problems / a find hit) requests a jump; the
  // editor watches this ticking target and scrolls there. A monotonically bumped
  // `nonce` makes a repeat jump to the same line fire again.
  let gotoTarget = $state<{ line: number; nonce: number } | null>(null);

  // Caret position (the editor pushes it here; the footer displays it).
  let caretLine = $state(1);
  let caretCol = $state(1);

  // "Reveal in project tree" relay — the toolbar's Select-opened-file button and
  // the palette bump this; the sidebar reacts by expanding + scrolling to it.
  let revealNonce = $state(0);

  return {
    get caretLine() { return caretLine; },
    get caretCol()  { return caretCol; },
    setCaret(line: number, col: number) { caretLine = line; caretCol = col; },

    get leftPanel()   { return leftPanel; },
    get rightPanel()  { return rightPanel; },
    get bottomPanel() { return bottomPanel; },
    get settingsOpen() { return settingsOpen; },
    get docsOpen()     { return docsOpen; },
    get paletteOpen()  { return paletteOpen; },
    get findOpen()     { return findOpen; },
    get gotoTarget()   { return gotoTarget; },
    get revealNonce()  { return revealNonce; },
    get treeExpanded() { return treeExpanded; },

    /** Toggle a left tool window (clicking the active one closes it). */
    toggleLeft(p: LeftPanel)  { leftPanel = leftPanel === p ? null : p; },
    /** Toggle a right tool window (clicking the active one closes it). */
    toggleRight(p: RightPanel) { rightPanel = rightPanel === p ? null : p; },
    /** Toggle a bottom dock section (clicking the active one closes the dock). */
    toggleBottom(p: BottomPanel) { bottomPanel = bottomPanel === p ? null : p; },
    /** Switch the bottom dock to a section (opening the dock if closed). */
    showBottom(p: BottomPanel) { bottomPanel = p; },
    /** Close the bottom dock entirely. */
    closeBottom() { bottomPanel = null; },
    /** Ensure a specific left tool is showing (used by "reveal in project"). */
    showLeft(p: LeftPanel) { leftPanel = p; },

    openSettings()  { settingsOpen = true; },
    closeSettings() { settingsOpen = false; },
    toggleDocs()    { docsOpen = !docsOpen; },
    closeDocs()     { docsOpen = false; },
    togglePalette() { paletteOpen = !paletteOpen; },
    closePalette()  { paletteOpen = false; },
    openFind()      { findOpen = true; },
    closeFind()     { findOpen = false; },

    /** Ask the editor to scroll to a 1-based line (a panel → editor relay). */
    requestGoto(line: number) {
      gotoTarget = { line, nonce: (gotoTarget?.nonce ?? 0) + 1 };
    },

    /** Reveal the active file in the Project tree (ensures Project is open + bumps
     *  the reveal relay the sidebar watches). */
    revealActiveInTree() {
      leftPanel = 'project';
      revealNonce += 1;
    },

    // ── Project-tree expansion (controlled) ──────────────────────────────────
    isExpanded(id: string): boolean { return treeExpanded.has(id); },
    setExpanded(id: string, next: boolean) {
      if (next) treeExpanded.add(id); else treeExpanded.delete(id);
    },
    /** Collapse every folder in the Project tree. */
    collapseAllTree() { treeExpanded.clear(); },
    /** Expand a set of ids (the sidebar computes the full folder id list). */
    expandTreeIds(ids: Iterable<string>) { for (const id of ids) treeExpanded.add(id); },
  };
}

export const bennuUiStore = createBennuUiStore();
