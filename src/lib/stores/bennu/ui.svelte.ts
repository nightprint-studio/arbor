/**
 * Bennu window UI store — chrome state for the standalone Java-editor window:
 * which left / right / bottom dockable panel is open, the tree expansion set for
 * the Project tool, and the docs/settings/palette/find overlay flags. Pure
 * session UI state (no persistence needed yet) — mirrors the merula window's
 * `merula-store` shape at a smaller scale.
 *
 * Tool-window layout (IntelliJ New UI):
 *   • LEFT rail (top)     — Project (tree), Structure (symbols), Dependencies.
 *   • LEFT rail (bottom)  — bottom-dock toggles: Build, Run, Problems, TODO, Terminal.
 *   • RIGHT rail          — Maven, Tests, Trees (top); the Forms toggle (bottom).
 *   • BOTTOM dock         — Build · Run · Problems · TODO · Forms · Terminal, one panel per
 *                           rail button (Build and Problems share one).
 *
 * Running something — a program, a debug session, a test run — is all one panel (Run), one tab
 * each. The Tests tool window on the right is the *catalogue*, not the runs.
 * Find-in-project is a modal (Ctrl+Shift+F), not a rail tool.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import type { FrameworkCatalogId } from '$lib/components/bennu/framework-catalogs';

import { SvelteSet } from 'svelte/reactivity';
import type { GenerateMode } from '$lib/components/bennu/bennu-intentions';

/** Left tool windows (activity bar, top group). */
export type LeftPanel = 'project' | 'structure' | 'dependencies';
/**
 * Right tool windows (activity bar).
 *
 * `tests` is the **catalogue** — every test the project declares, sortable and filterable, with
 * a run button per row. What a run *did* is deliberately not here: that is an event, and it
 * lives as a tab of the Run console beside the other things you have launched.
 *
 * `ast` is the syntax tree of the buffer in front of you — what the grammar actually built,
 * which is the only thing that answers "why did it parse that way".
 */
// `maven` and `cargo` share the rail slot and the Alt+8 keybinding: they are the same tool window
// for two ecosystems, and a project is only ever one of them.
/** Right tool windows — a split beside the editor.
 *
 *  `i18n` is the one with no rail button, for the same reason `hierarchy` has none in the bottom dock:
 *  it is about the caret, and on every file that is not a fulcrum translation bundle it could only ever
 *  say "not here". It is reached from the editor's own toolbar — which appears on a bundle and nowhere
 *  else — from the palette, and from its shortcut. */
export type RightPanel =
  | 'maven' | 'cargo' | 'tests' | 'ast' | 'i18n'
  /** A view a plugin registered with `arbor.ui.add_view`, keyed `plugin:<plugin>:<view_id>`.
   *
   *  Bennu puts plugin views here rather than in the body, and that is a real choice about
   *  what a view is FOR in a code editor: Corvus's body is a commit graph you look at
   *  instead of the diff, Bennu's body is the file you are editing. A shader preview, a
   *  rendered markdown, a live query result — all of them are worth nothing if opening them
   *  hides the text they are about. Beside the editor, resizable, is the split. */
  | `plugin:${string}`;
/** Bottom tool windows — one panel per rail button, except Build and Problems which share
 *  one. The Forms inspector lives here (wide, horizontal data) rather than in a narrow side
 *  panel; its toggle sits in the right rail's bottom cluster.
 *
 *  The framework catalogs arrive as {@link FrameworkCatalogId} rather than being listed again:
 *  they are declared once, in `framework-catalogs.ts`, and the two lists had already drifted apart
 *  once — a catalog added there and forgotten here is one the dock cannot be told to open, which
 *  the compiler only notices at the call site that tries.
 *
 *  Most of them are palette-only: a framework tool is noise on the projects that don't use it and
 *  the rail is the one piece of chrome that is always on screen. Which of them earn a rail button
 *  is that table's `rail` flag, not a distinction this type makes. */
export type BottomPanel =
  | 'problems'
  | 'terminal'
  | 'build'
  /** The launched program's console — its own tool window, not a section of Build: a build
   *  log is finished when you read it, a program's output is live, typed into and stopped.
   *
   *  This is where **tests** and the **debugger** live too. All three are the same activity —
   *  you started something and you are watching it — so they share one panel, one Stop button
   *  and one transcript, and the tab strip says which of them you are looking at. See
   *  {@link RunTab}. */
  | 'run'
  /** The Lua log stream. A bottom panel and not an overlay, for the same reason Corvus docks
   *  it there: you read it WHILE looking at something else — a plugin that failed to start,
   *  a form that did nothing — and a panel that covers the thing you are diagnosing is a
   *  panel you have to keep closing. */
  | 'plugin-logs'
  | 'todos'
  /** The call / type hierarchy — a tree of callers, callees, supertypes or implementors.
   *
   *  Here rather than in a side rail because of the shape of a row: a name, a file and the line of
   *  source around it. That is wide data, like Problems and TODO, and in a narrow column it would
   *  lose the preview — which is the part that lets a caller be recognised without opening it.
   *
   *  Deliberately without a rail button: it is opened by an action about the caret (there is nothing
   *  to show until something has been asked), so it is reached from the palette and its shortcuts,
   *  and closed from its own header. */
  | 'hierarchy'
  | 'forms'
  | FrameworkCatalogId;

/** Which tab the Go-to navigator opens on. */
export type NavMode = 'class' | 'file' | 'symbol' | 'all';

/**
 * Which tab of the Run console is showing.
 *
 * `'tests'` is the test run's tree; anything else is a run id from `bennuRunStore` — one tab per
 * launched program, the debugger's session being one of them. Both kinds appear because you ran
 * something and go away when you close them.
 *
 * There is one test tab rather than one per test run, because `bennuTestStore` holds one run:
 * its tree, its filters and its counters are singletons. When it grows a run history this
 * becomes several ids and nothing else here has to change.
 *
 * It lives in this store rather than in the panel because it is addressable from outside —
 * starting a test run means "the Run console, on that tab", and a panel-local variable cannot
 * be told that.
 */
export type RunTab = 'tests' | (string & {});

function createBennuUiStore() {
  // Default the Project tool open so the shell shows the tree on launch.
  let leftPanel = $state<LeftPanel | null>('project');
  let rightPanel = $state<RightPanel | null>(null);
  let bottomPanel = $state<BottomPanel | null>(null);
  /** `'tests'`, or a `bennuRunStore` run id. `null` = whichever run tab is active. */
  let runTab = $state<RunTab | null>(null);

  let settingsOpen = $state(false);
  /** A specific Settings page to land on, when something opened it *for a reason*. */
  let settingsSection = $state<string | null>(null);
  let docsOpen = $state(false);
  /** The dialog that rearranges the two activity rails. */
  let customizeRailsOpen = $state(false);
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
  // The breakpoint list — every breakpoint of the project in one place: disable one, drop one,
  // or add an exception breakpoint, which the gutter has nowhere to express.
  let breakpointsOpen = $state(false);
  // Structural search & replace (Ctrl+Shift+M) — find code by its SHAPE, count it, rewrite it.
  // A modal rather than a panel: it is a thing you go and do, with a wide answer, and it does not
  // belong open beside the editor the way a tool window does.
  let ssrOpen = $state(false);
  // Go-to navigator — one overlay over classes / files / symbols; the shortcut that opened it
  // picks the starting tab (Ctrl+N = class, Ctrl+Shift+N = file, Ctrl+Shift+Y = symbol).
  let navOpen = $state(false);
  let navMode = $state<NavMode>('class');
  // Index inspector modal (debug: index stats + class list).
  let indexInspectorOpen = $state(false);
  // The module-graph window (who depends on whom, inside the project).
  let moduleGraphOpen = $state(false);
  // Project mojibake-scan modal (whole-project UTF-8-as-Cp1252 corruption report).
  let mojibakeScanOpen = $state(false);
  // Tomcat hot-swap settings modal (link a Tomcat + pick the deployed webapp).
  let tomcatConfigOpen = $state(false);
  // File Structure popup (Ctrl+F12) — a searchable quick-outline of the active file.
  let fileStructureOpen = $state(false);
  // About Bennu modal.
  let aboutOpen = $state(false);

  // ── Tools ─────────────────────────────────────────────────────────────────
  // The same three Corvus keeps under its hamburger's Tools separator. They belong here too
  // now that bennu hosts plugins: a product that runs plugins has to be able to install one,
  // see which are loaded, and read why one did not start.
  let pluginsOpen = $state(false);
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
  // "Add dependency" (runs `cargo add`). In the store rather than in the Cargo panel because it is
  // reachable from the palette, and the panel it is launched from need not be open for that.
  let cargoAddOpen = $state(false);
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

  // "Reveal in project tree" relay — the toolbar's Select-opened-file button, the palette and
  // the build-unit panels bump this; the sidebar reacts by expanding + scrolling to it.
  //
  // `path` names WHAT to reveal, `null` meaning "whatever file is open" — the two are one relay
  // because they are one action from the sidebar's side, and the nonce is what makes asking twice
  // for the same target fire twice.
  let revealTarget = $state<{ path: string | null; nonce: number }>({ path: null, nonce: 0 });

  return {
    get caretLine() { return caretLine; },
    get caretCol()  { return caretCol; },
    setCaret(line: number, col: number) { caretLine = line; caretCol = col; },

    get leftPanel()   { return leftPanel; },
    get rightPanel()  { return rightPanel; },
    get bottomPanel() { return bottomPanel; },
    get settingsOpen() { return settingsOpen; },
    get docsOpen()     { return docsOpen; },
    get customizeRailsOpen() { return customizeRailsOpen; },
    get paletteOpen()  { return paletteOpen; },
    get findOpen()     { return findOpen; },
    get projectConfigOpen() { return projectConfigOpen; },
    get runConfigOpen() { return runConfigOpen; },
    get breakpointsOpen() { return breakpointsOpen; },
    get ssrOpen() { return ssrOpen; },
    get navOpen()      { return navOpen; },
    get navMode()      { return navMode; },
    /** Query to pre-fill the Find-in-project field with (from a selection), or ''. */
    get findInitial()  { return findInitial; },
    /** Query to pre-fill the Go-to navigator field with (from a selection), or ''. */
    get navInitial()   { return navInitial; },
    get indexInspectorOpen() { return indexInspectorOpen; },
    get moduleGraphOpen() { return moduleGraphOpen; },
    get mojibakeScanOpen() { return mojibakeScanOpen; },
    get tomcatConfigOpen() { return tomcatConfigOpen; },
    get fileStructureOpen() { return fileStructureOpen; },
    get aboutOpen()    { return aboutOpen; },
    get pluginsOpen()     { return pluginsOpen; },
    get generateOpen() { return generateOpen; },
    get jpaGenerateOpen() { return jpaGenerateOpen; },
    get jpaGenerateFile() { return jpaGenerateFile; },
    get jpaGenerateAction() { return jpaGenerateAction; },
    get generateMode() { return generateMode; },
    get validationCreatorOpen() { return validationCreatorOpen; },
    get workspaceManagerOpen() { return workspaceManagerOpen; },
    get cargoAddOpen() { return cargoAddOpen; },
    get gotoTarget()   { return gotoTarget; },
    get gotoOffsetTarget() { return gotoOffsetTarget; },
    get revealTarget() { return revealTarget; },
    get treeExpanded() { return treeExpanded; },

    /** Toggle a left tool window (clicking the active one closes it). */
    toggleLeft(p: LeftPanel)  { leftPanel = leftPanel === p ? null : p; },
    /** Toggle a right tool window (clicking the active one closes it). */
    toggleRight(p: RightPanel) { rightPanel = rightPanel === p ? null : p; },
    /** Open one, without the toggle. `arbor.ui.open_panel` means open — a plugin that asks
     *  twice (a second click on its toolbar button) must not be answered by closing. */
    showRight(p: RightPanel) { rightPanel = p; },
    /** Close whatever is in the right split. The counterpart to `showRight` for a panel that
     *  owns a close button and must not have to know which panel it is. */
    closeRight() { rightPanel = null; },
    /** Toggle a bottom dock section (clicking the active one closes the dock). */
    toggleBottom(p: BottomPanel) { bottomPanel = bottomPanel === p ? null : p; },
    /** Switch the bottom dock to a section (opening the dock if closed). */
    showBottom(p: BottomPanel) { bottomPanel = p; },

    /** Which tab of the Run console is showing — see {@link RunTab}. `null` = the active run. */
    get runTab() { return runTab; },
    /** Show a tab of the Run console. Pass `null` to follow the active run again. */
    showRunTab(t: RunTab | null) { runTab = t; },
    /**
     * Open the Run console on the test run's tab — what starting a run means. Not what
     * <kbd>Alt</kbd>+<kbd>5</kbd> means: that opens the **catalogue** (`toggleRight('tests')`),
     * which is where a run is started from and exists whether or not one has ever happened.
     */
    showTestRun() { bottomPanel = 'run'; runTab = 'tests'; },
    /** Close the bottom dock entirely. */
    closeBottom() { bottomPanel = null; },
    /** Ensure a specific left tool is showing (used by "reveal in project"). */
    showLeft(p: LeftPanel) { leftPanel = p; },

    /**
     * Close any open tool window whose rail icon has just disappeared.
     *
     * Called when the active project switches to one that doesn't offer a tool (a Cargo
     * project has no Structure / Maven / Dependencies / Forms — see
     * `BennuWindow`'s `javaTools`). Without this a panel opened on a Java project would
     * survive the switch with no way left to close it: its toggle is gone from both the
     * rail and the palette. Left falls back to Project rather than to nothing, so the
     * side rail never reads as broken.
     */
    dropUnavailablePanels(keep: { left: LeftPanel[]; right: RightPanel[]; bottom: BottomPanel[] }) {
      if (leftPanel && !keep.left.includes(leftPanel)) leftPanel = 'project';
      // Plugin views are exempt: what a plugin offers is decided by the plugin's own targets
      // and by whether it is enabled, never by whether the project is Maven or Cargo. Listing
      // them in `keep.right` would mean the caller enumerating packages it cannot know.
      if (rightPanel && !rightPanel.startsWith('plugin:') && !keep.right.includes(rightPanel)) {
        rightPanel = null;
      }
      if (bottomPanel && !keep.bottom.includes(bottomPanel)) bottomPanel = null;
    },

    /** Open Settings, optionally landing on a specific page.
     *
     *  The section argument is what lets a failure report *be* its own fix: a status-bar pill
     *  saying "rust-analyzer is not running" opens the page that can install or re-point it,
     *  rather than the page the user last happened to be on. */
    openSettings(section?: string) {
      if (section) settingsSection = section;
      settingsOpen = true;
    },
    closeSettings() { settingsOpen = false; settingsSection = null; },
    /** The page Settings should land on, consumed once by the modal. */
    get settingsSection() { return settingsSection; },
    /** Clear the requested page, so re-opening Settings normally stays where the user was. */
    consumeSettingsSection() { settingsSection = null; },
    toggleDocs()    { docsOpen = !docsOpen; },
    closeDocs()     { docsOpen = false; },
    openCustomizeRails()  { customizeRailsOpen = true; },
    closeCustomizeRails() { customizeRailsOpen = false; },
    togglePalette() { paletteOpen = !paletteOpen; },
    closePalette()  { paletteOpen = false; },
    /** Open Find-in-project, optionally pre-filling the query (e.g. the editor selection). */
    openFind(initial = '')      { findInitial = initial; findOpen = true; },
    closeFind()     { findOpen = false; },

    openProjectConfig()  { projectConfigOpen = true; },
    closeProjectConfig() { projectConfigOpen = false; },
    openRunConfig()      { runConfigOpen = true; },
    closeRunConfig()     { runConfigOpen = false; },
    openBreakpoints()    { breakpointsOpen = true; },
    closeBreakpoints()   { breakpointsOpen = false; },
    openSsr()            { ssrOpen = true; },
    closeSsr()           { ssrOpen = false; },
    /** Open the Go-to navigator on `mode`'s tab, optionally pre-filling the query (e.g. the
     *  editor selection). Every tab is reachable with Tab once it is open. */
    openNav(mode: NavMode, initial = '') { navMode = mode; navInitial = initial; navOpen = true; },
    closeNav()           { navOpen = false; },
    openIndexInspector() { indexInspectorOpen = true; },
    closeIndexInspector() { indexInspectorOpen = false; },
    openModuleGraph() { moduleGraphOpen = true; },
    closeModuleGraph() { moduleGraphOpen = false; },
    openMojibakeScan() { mojibakeScanOpen = true; },
    closeMojibakeScan() { mojibakeScanOpen = false; },
    openTomcatConfig() { tomcatConfigOpen = true; },
    closeTomcatConfig() { tomcatConfigOpen = false; },
    openFileStructure() { fileStructureOpen = true; },
    closeFileStructure() { fileStructureOpen = false; },
    openAbout()          { aboutOpen = true; },
    closeAbout()         { aboutOpen = false; },
    togglePlugins()      { pluginsOpen = !pluginsOpen; },
    closePlugins()       { pluginsOpen = false; },
    /** Docked in the bottom tool window, not floated: the whole use of a log is reading it
     *  while looking at the thing it is about. */
    togglePluginLogs()   { bottomPanel = bottomPanel === 'plugin-logs' ? null : 'plugin-logs'; },
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
    /** Open / close the Add-dependency dialog (`cargo add`). */
    openCargoAdd()  { cargoAddOpen = true; },
    closeCargoAdd() { cargoAddOpen = false; },

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
      revealTarget = { path: null, nonce: revealTarget.nonce + 1 };
    },

    /**
     * Put the Project tree **on** a path — expanded, selected, scrolled to, and holding the
     * keyboard focus.
     *
     * What the Cargo and Dependencies panels mean by "Focus in Project": those panels list the
     * project by build unit, and the question they leave you with is where that crate or module
     * actually lives. Distinct from {@link revealActiveInTree}, which follows the editor and
     * takes no argument — here the target is a directory nobody has opened, and probably cannot
     * open, because it is a folder.
     */
    focusInTree(path: string) {
      leftPanel = 'project';
      revealTarget = { path, nonce: revealTarget.nonce + 1 };
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
