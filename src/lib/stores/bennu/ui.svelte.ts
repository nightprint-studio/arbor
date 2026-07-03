/**
 * Bennu window UI store — chrome state for the standalone Java-editor window:
 * which left / right / bottom dockable panel is open, the tree expansion set for
 * the Project tool, and the docs/settings/palette/find overlay flags. Pure
 * session UI state (no persistence needed yet) — mirrors the merula window's
 * `merula-store` shape at a smaller scale.
 *
 * Tool-window layout (IntelliJ New UI):
 *   • LEFT rail (top)     — Project (tree), Structure (symbols), Dependencies.
 *   • LEFT rail (bottom)  — the bottom-dock toggles: Terminal, Problems.
 *   • RIGHT rail          — Maven (top), Services/Run (bottom). Mock panels.
 *   • BOTTOM dock         — Problems + Terminal, tabbed. Its toggles live in the
 *                           left rail's bottom cluster.
 * Find-in-project is a modal (Ctrl+Shift+F), not a rail tool.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { SvelteSet } from 'svelte/reactivity';
import type { GenerateMode } from '$lib/components/bennu/bennu-intentions';

/** Left tool windows (activity bar, top group). */
export type LeftPanel = 'project' | 'structure' | 'dependencies';
/** Right tool windows (activity bar) — mock tool panels for now. */
export type RightPanel = 'maven' | 'services';
/** Bottom dock sections (tabbed). */
export type BottomPanel = 'problems' | 'terminal' | 'build' | 'todos';

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

  // Per-project configuration modal (JDK / encoding / roots / modules).
  let projectConfigOpen = $state(false);
  // Run-configuration modal (main class for `java -cp … <mainClass>`) — there's no
  // main-class discovery yet, so ▶ Run without a remembered class opens this.
  let runConfigOpen = $state(false);
  // Go-to navigator (Ctrl+N = class, Ctrl+Shift+N = file) — a filterable quick-open.
  let navOpen = $state(false);
  let navMode = $state<'class' | 'file'>('class');
  // Index inspector modal (debug: index stats + class list).
  let indexInspectorOpen = $state(false);
  // About Bennu modal.
  let aboutOpen = $state(false);
  // Generate modal (constructor / getters / setters) + its preselected mode
  // (Alt+Insert opens it fresh; an Alt+Enter "Generate…" intention preselects one).
  let generateOpen = $state(false);
  let generateMode = $state<GenerateMode>('getters-setters');
  // The intentions overlay (Alt+Enter) owns its own visibility in
  // `bennuIntentionsStore`; the window mounts it unconditionally. No flag needed
  // here — the openers below delegate to that store.

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
    get projectConfigOpen() { return projectConfigOpen; },
    get runConfigOpen() { return runConfigOpen; },
    get navOpen()      { return navOpen; },
    get navMode()      { return navMode; },
    get indexInspectorOpen() { return indexInspectorOpen; },
    get aboutOpen()    { return aboutOpen; },
    get generateOpen() { return generateOpen; },
    get generateMode() { return generateMode; },
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

    openProjectConfig()  { projectConfigOpen = true; },
    closeProjectConfig() { projectConfigOpen = false; },
    openRunConfig()      { runConfigOpen = true; },
    closeRunConfig()     { runConfigOpen = false; },
    /** Open the Go-to navigator in `mode` ('class' | 'file'). */
    openNav(mode: 'class' | 'file') { navMode = mode; navOpen = true; },
    closeNav()           { navOpen = false; },
    openIndexInspector() { indexInspectorOpen = true; },
    closeIndexInspector() { indexInspectorOpen = false; },
    openAbout()          { aboutOpen = true; },
    closeAbout()         { aboutOpen = false; },
    /** Open the Generate modal, optionally preselecting a mode (an Alt+Enter
     *  "Generate…" intention routes here with the matching mode; Alt+Insert opens
     *  it with the last/default mode). */
    openGenerate(mode?: GenerateMode) {
      if (mode) generateMode = mode;
      generateOpen = true;
    },
    closeGenerate()      { generateOpen = false; },

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
