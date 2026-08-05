/**
 * Bennu window UI store — chrome state for the standalone Java-editor window:
 * which left / right / bottom dockable panel is open, the tree expansion set for
 * the Project tool, and the docs/settings/palette/find overlay flags. Pure
 * session UI state (no persistence needed yet) — mirrors the merula window's
 * `merula-store` shape at a smaller scale.
 *
 * Tool-window layout (IntelliJ New UI):
 *   • LEFT rail (top)     — Project (tree), Structure (symbols), Dependencies.
 *   • LEFT rail (bottom)  — bottom-dock toggles: Build, Tests, Problems, TODO, Terminal.
 *   • RIGHT rail          — Maven (top); Services + the Forms toggle (bottom).
 *   • BOTTOM dock         — Build · Tests · Problems · TODO · Forms · Terminal, tabbed.
 *                           Toggles live in the left rail (+ Forms in the right rail bottom).
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
/** Bottom tool windows — one panel per rail button, except Build and Problems which share
 *  one. The Forms inspector lives here (wide, horizontal data) rather than in a narrow side
 *  panel; its toggle sits in the right rail's bottom cluster.
 *
 *  The framework catalogs (see `framework-catalogs.ts`) are palette-only apart from
 *  `endpoints`: a framework tool is noise on the projects that don't use it and the rail is
 *  the one piece of chrome that is always on screen. A route list is the exception — it is
 *  something you keep open while working, not something you go and fetch. */
export type BottomPanel =
  | 'problems'
  | 'terminal'
  | 'build'
  | 'tests'
  | 'todos'
  | 'forms'
  | 'beans'
  | 'librarybeans'
  | 'endpoints'
  | 'springconfig'
  | 'springbindings'
  | 'springdocumented'
  | 'jpaentities'
  | 'jparepositories';

/** Which tab the Go-to navigator opens on. */
export type NavMode = 'class' | 'file' | 'symbol' | 'all';

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
  // Initial query seeded into the Find / Go-to fields when opened from a selection
  // (Ctrl+Shift+F / Ctrl+N / Ctrl+Shift+N with a word highlighted). '' → open empty.
  let findInitial = $state('');
  let navInitial = $state('');

  // Per-project configuration modal (JDK / encoding / roots / modules).
  let projectConfigOpen = $state(false);
  // Run-configuration modal (main class for `java -cp … <mainClass>`) — there's no
  // main-class discovery yet, so ▶ Run without a remembered class opens this.
  let runConfigOpen = $state(false);
  // Go-to navigator — one overlay over classes / files / symbols; the shortcut that opened it
  // picks the starting tab (Ctrl+N = class, Ctrl+Shift+N = file, Ctrl+Shift+Y = symbol).
  let navOpen = $state(false);
  let navMode = $state<NavMode>('class');
  // Index inspector modal (debug: index stats + class list).
  let indexInspectorOpen = $state(false);
  // Project mojibake-scan modal (whole-project UTF-8-as-Cp1252 corruption report).
  let mojibakeScanOpen = $state(false);
  // Tomcat hot-swap settings modal (link a Tomcat + pick the deployed webapp).
  let tomcatConfigOpen = $state(false);
  // File Structure popup (Ctrl+F12) — a searchable quick-outline of the active file.
  let fileStructureOpen = $state(false);
  // About Bennu modal.
  let aboutOpen = $state(false);
  // Generate modal (constructor / getters / setters) + its preselected mode
  // (Alt+Insert opens it fresh; an Alt+Enter "Generate…" intention preselects one).
  let generateOpen = $state(false);
  let generateMode = $state<GenerateMode>('getters-setters');
  // JPA generation (repository / projection / query method). Its own modal rather than a mode of
  // the one above: that one rewrites the class you are in, this one builds against the whole
  // entity model and often writes a different file.
  let jpaGenerateOpen = $state(false);
  /** The file the JPA form should start from — the entity or repository you had open when you
   *  pressed the button. Without it the form opens on whichever entity happens to be first,
   *  which is never the one you were looking at. */
  let jpaGenerateFile = $state<string | null>(null);
  /** Which generation was asked for — the id of a backend-contributed action (`jpa.query.list`).
   *  Held as a string so the store stays free of the component layer's table, and so an
   *  extension can contribute an action id this store has never heard of. */
  let jpaGenerateAction = $state('jpa.query.list');
  // "New validator" modal (opened from the Struts-validation-file editor toolbar).
  let validationCreatorOpen = $state(false);
  // Workspace manager modal (create / rename / recolor / delete workspaces, manage members).
  let workspaceManagerOpen = $state(false);
  // The intentions overlay (Alt+Enter) owns its own visibility in
  // `bennuIntentionsStore`; the window mounts it unconditionally. No flag needed
  // here — the openers below delegate to that store.

  // Project-tree expansion set (controlled Tree expansion) so the toolbar can
  // Collapse-all / Expand-all and Select-opened-file can reveal a path.
  const treeExpanded = new SvelteSet<string>();

  // Goto relay — a panel (Structure / Problems / a find hit) requests a jump; the
  // editor watches this ticking target and scrolls there. A monotonically bumped
  // `nonce` makes a repeat jump to the same line fire again.
  /** How long a go-to may take before it is announced. Under this it lands as if
   *  instantly and an indicator would only flicker. */
  const NAVIGATION_ANNOUNCE_MS = 250;
  /** What go-to is resolving right now (`"List"`), or `null`. */
  let navigatingTo = $state<string | null>(null);
  /** Bumped per navigation, so a stale one cannot clear a newer one's label. */
  let navigationToken = 0;
  let navigationTimer: ReturnType<typeof setTimeout> | null = null;

  let gotoTarget = $state<{ line: number; nonce: number } | null>(null);

  // Goto-by-byte-offset relay — the Forms tool window (a sibling of the editor) asks the
  // editor to move the caret to a UTF-8 byte offset (a `<form>` tag / field-name span).
  // Same shape as `gotoTarget`: the editor watches the ticking target and scrolls there,
  // and the bumped `nonce` re-fires a repeat jump to the same offset.
  let gotoOffsetTarget = $state<{ offset: number; nonce: number } | null>(null);

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
    /** Query to pre-fill the Find-in-project field with (from a selection), or ''. */
    get findInitial()  { return findInitial; },
    /** Query to pre-fill the Go-to navigator field with (from a selection), or ''. */
    get navInitial()   { return navInitial; },
    get indexInspectorOpen() { return indexInspectorOpen; },
    get mojibakeScanOpen() { return mojibakeScanOpen; },
    get tomcatConfigOpen() { return tomcatConfigOpen; },
    get fileStructureOpen() { return fileStructureOpen; },
    get aboutOpen()    { return aboutOpen; },
    get generateOpen() { return generateOpen; },
    get jpaGenerateOpen() { return jpaGenerateOpen; },
    get jpaGenerateFile() { return jpaGenerateFile; },
    get jpaGenerateAction() { return jpaGenerateAction; },
    get generateMode() { return generateMode; },
    get validationCreatorOpen() { return validationCreatorOpen; },
    get workspaceManagerOpen() { return workspaceManagerOpen; },
    get gotoTarget()   { return gotoTarget; },
    get gotoOffsetTarget() { return gotoOffsetTarget; },
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

    /**
     * Close any open tool window whose rail icon has just disappeared.
     *
     * Called when the active project switches to one that doesn't offer a tool (a Cargo
     * project has no Structure / Maven / Dependencies / Services / Forms — see
     * `BennuWindow`'s `javaTools`). Without this a panel opened on a Java project would
     * survive the switch with no way left to close it: its toggle is gone from both the
     * rail and the palette. Left falls back to Project rather than to nothing, so the
     * side rail never reads as broken.
     */
    dropUnavailablePanels(keep: { left: LeftPanel[]; right: RightPanel[]; bottom: BottomPanel[] }) {
      if (leftPanel && !keep.left.includes(leftPanel)) leftPanel = 'project';
      if (rightPanel && !keep.right.includes(rightPanel)) rightPanel = null;
      if (bottomPanel && !keep.bottom.includes(bottomPanel)) bottomPanel = null;
    },

    openSettings()  { settingsOpen = true; },
    closeSettings() { settingsOpen = false; },
    toggleDocs()    { docsOpen = !docsOpen; },
    closeDocs()     { docsOpen = false; },
    togglePalette() { paletteOpen = !paletteOpen; },
    closePalette()  { paletteOpen = false; },
    /** Open Find-in-project, optionally pre-filling the query (e.g. the editor selection). */
    openFind(initial = '')      { findInitial = initial; findOpen = true; },
    closeFind()     { findOpen = false; },

    openProjectConfig()  { projectConfigOpen = true; },
    closeProjectConfig() { projectConfigOpen = false; },
    openRunConfig()      { runConfigOpen = true; },
    closeRunConfig()     { runConfigOpen = false; },
    /** Open the Go-to navigator on `mode`'s tab, optionally pre-filling the query (e.g. the
     *  editor selection). Every tab is reachable with Tab once it is open. */
    openNav(mode: NavMode, initial = '') { navMode = mode; navInitial = initial; navOpen = true; },
    closeNav()           { navOpen = false; },
    openIndexInspector() { indexInspectorOpen = true; },
    closeIndexInspector() { indexInspectorOpen = false; },
    openMojibakeScan() { mojibakeScanOpen = true; },
    closeMojibakeScan() { mojibakeScanOpen = false; },
    openTomcatConfig() { tomcatConfigOpen = true; },
    closeTomcatConfig() { tomcatConfigOpen = false; },
    openFileStructure() { fileStructureOpen = true; },
    closeFileStructure() { fileStructureOpen = false; },
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
    /** `action` is a contributed action id — the kind is chosen before the dialog opens, so the
     *  dialog has one job and a title that names it. */
    openJpaGenerate(action: string, fromFile?: string | null) {
      jpaGenerateAction = action;
      jpaGenerateFile = fromFile ?? null;
      jpaGenerateOpen = true;
    },
    closeJpaGenerate()   { jpaGenerateOpen = false; jpaGenerateFile = null; },
    /** Open the "New validator" modal (from the validation-file editor toolbar). */
    openValidationCreator()  { validationCreatorOpen = true; },
    closeValidationCreator() { validationCreatorOpen = false; },
    /** Open / close the workspace manager modal. */
    openWorkspaceManager()  { workspaceManagerOpen = true; },
    closeWorkspaceManager() { workspaceManagerOpen = false; },

    /** What go-to is currently resolving, or `null` when nothing is. Drives the status
     *  bar's "Opening …" item.
     *
     *  Go-to is the one action here that can take seconds without anything appearing:
     *  a library type has to be found on the classpath, its source or bytecode read out
     *  of an archive and a view written to disk, and until that finishes the editor
     *  looks exactly as it did before the click. Silence reads as "nothing happened",
     *  which is why it gets said out loud. */
    get navigatingTo() {
      return navigatingTo;
    },

    /** Mark go-to as in progress, naming what it is opening. Returns the token to hand
     *  {@link endNavigation}, so a slow resolution that finishes AFTER a newer one
     *  started cannot clear the newer one's label.
     *
     *  The label appears only if the navigation is still running a moment later. Most
     *  land immediately, and an indicator that appears and vanishes inside a frame is
     *  noise rather than feedback — only a go-to that is actually making you wait says
     *  anything. */
    beginNavigation(label: string): number {
      navigationToken += 1;
      const token = navigationToken;
      if (navigationTimer !== null) clearTimeout(navigationTimer);
      navigationTimer = setTimeout(() => {
        if (token === navigationToken) navigatingTo = label;
      }, NAVIGATION_ANNOUNCE_MS);
      return token;
    },

    /** Clear the in-progress mark, if `token` is still the current navigation. */
    endNavigation(token: number) {
      if (token !== navigationToken) return;
      if (navigationTimer !== null) {
        clearTimeout(navigationTimer);
        navigationTimer = null;
      }
      navigatingTo = null;
    },

    /** Ask the editor to scroll to a 1-based line (a panel → editor relay). */
    requestGoto(line: number) {
      gotoTarget = { line, nonce: (gotoTarget?.nonce ?? 0) + 1 };
    },

    /** Ask the editor to move the caret to a **UTF-8 byte offset** and reveal it (the
     *  Forms tool window → editor relay, for jumping to a `<form>` tag / field name). */
    requestGotoOffset(offset: number) {
      gotoOffsetTarget = { offset, nonce: (gotoOffsetTarget?.nonce ?? 0) + 1 };
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
