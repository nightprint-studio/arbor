<script lang="ts">
  /**
   * BennuWindow — the standalone Java-editor window shell.
   *
   * Boots the theme/appearance/animation config locally (each window is its own JS
   * context, so AppShell's onMount never runs here — mirrors MerulaWindow), then
   * composes Arbor's standard IntelliJ-New-UI frame like Corvus/Merula:
   *   TitleBar (project · … · run/debug · palette · docs · settings) + a bg-elevated
   *   WorkspaceShell with left/right activity rails + floating bg-base panel cards +
   *   a BOTTOM dock (one panel per rail button) + the footer status bar.
   *
   * Tool windows (IntelliJ New UI):
   *   • LEFT rail top     — Project (tree), Structure (symbols), Dependencies — left side panels.
   *   • LEFT rail bottom  — the bottom-dock toggles (Build, Problems, TODO, Terminal). Docs &
   *                         Settings live in the titlebar's right cluster.
   *   • RIGHT rail        — Maven (top); the Forms toggle (bottom).
   *   • BOTTOM dock       — one panel per rail button, each owning its header and its actions:
   *                         Build+Problems (two views of one run, sharing a panel) · TODO ·
   *                         Forms · Terminal.
   * Find-in-project is a modal (Ctrl+Shift+F / palette), not a rail tool.
   */
  import { onMount, untrack } from 'svelte';
  import {
    Command, FolderTree, ListTree, Search, Hash, FileCode2, AlertTriangle,
    TerminalSquare, Hammer, Server, Wand2, Lightbulb, SlidersHorizontal, Info,
    Library, Target, Play, ListTodo, Box, RotateCw, IndentIncrease, ShieldCheck,
    TextCursorInput, ListChecks, BookOpen, FlaskConical, ListRestart, Bug, Braces, Languages,
    Cog, Network,
  } from 'lucide-svelte';

  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { signalWindowReady } from '$lib/ipc/window';
  import { syncWindowTitle } from '$lib/utils/window-title.svelte';
  import { isKey } from '$lib/utils/keybindings';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { profileStore } from '$lib/stores/profiles.svelte';
  import { recordRecentProject, onOpenIntent } from '$lib/ipc/recents';

  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import FeedbackHost from '$lib/feedback/FeedbackHost.svelte';
  import FeedbackStatusButtons from '$lib/feedback/FeedbackStatusButtons.svelte';
  import CommandPaletteShell, { type PaletteSection } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import type { IconComponent } from '$lib/types/icon';

  import BennuTitleBar from './BennuTitleBar.svelte';
  import BennuStatusBar from './BennuStatusBar.svelte';
  import BennuSidebar from './BennuSidebar.svelte';
  import BennuStructurePanel from './BennuStructurePanel.svelte';
  import BennuDependenciesPanel from './BennuDependenciesPanel.svelte';
  import BennuMavenPanel from './BennuMavenPanel.svelte';
  import BennuCargoPanel from './BennuCargoPanel.svelte';
  import BennuTestsCatalogPanel from './BennuTestsCatalogPanel.svelte';
  import BennuCargoTestsPanel from './BennuCargoTestsPanel.svelte';
  import RustTestIcon from './RustTestIcon.svelte';
  import SyntaxTreePanel from '$lib/components/shared/internal/SyntaxTreePanel.svelte';
  import { bennuAstStore } from '$lib/stores/bennu/ast.svelte';
  import MavenIcon from './MavenIcon.svelte';
  import JUnitIcon from './JUnitIcon.svelte';
  import BennuBottomDock from './BennuBottomDock.svelte';
  import BennuEditor from './BennuEditor.svelte';
  import BennuDocsPanel from './BennuDocsPanel.svelte';
  import BennuSettingsModal from './BennuSettingsModal.svelte';
  import BennuFindInFilesModal from './BennuFindInFilesModal.svelte';
  import BennuProjectConfigModal from './BennuProjectConfigModal.svelte';
  import BennuAboutModal from './BennuAboutModal.svelte';
  import BennuGenerateModal from './BennuGenerateModal.svelte';
  import BennuJpaGenerateModal from './BennuJpaGenerateModal.svelte';
  import { JPA_PALETTE_ACTIONS } from './jpa-actions';
  import BennuValidationModal from './BennuValidationModal.svelte';
  import BennuWorkspaceManagerModal from './BennuWorkspaceManagerModal.svelte';
  import BennuCargoAddModal from './BennuCargoAddModal.svelte';
  import BennuIntentionsOverlay from './BennuIntentionsOverlay.svelte';
  import BennuExternalChangeModal from './BennuExternalChangeModal.svelte';
  import BennuRunConfigModal from './BennuRunConfigModal.svelte';
  import BennuBreakpointsModal from './BennuBreakpointsModal.svelte';
  import BennuSsrModal from './BennuSsrModal.svelte';
  import BennuRenameModal from './BennuRenameModal.svelte';
  import BennuUsagesPopover from './BennuUsagesPopover.svelte';
  import BennuGotoModal from './BennuGotoModal.svelte';
  import BennuIndexInspectorModal from './BennuIndexInspectorModal.svelte';
  import BennuModuleGraphModal from './BennuModuleGraphModal.svelte';
  import BennuMojibakeScanModal from './BennuMojibakeScanModal.svelte';
  import BennuTomcatConfigModal from './BennuTomcatConfigModal.svelte';
  // The job-output bottom panel: shared chrome (lives under corvus/jobs/ but depends only on the
  // shared jobsStore + uiStore — the same one FeedbackHost's JobsOverlay uses). Mounted here so
  // "view output" from Bennu's Jobs overlay opens the panel instead of doing nothing.
  import JobOutputPanel from '$lib/components/corvus/jobs/JobOutputPanel.svelte';
  import { uiStore as sharedUiStore } from '$lib/stores/ui.svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import BennuFileStructureModal from './BennuFileStructureModal.svelte';
  import type { GenerateMode } from './bennu-intentions';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { workspacesStore } from '$lib/stores/bennu/workspaces.svelte';
  import { isJavaFile, isJspFile, isLspFile, supportsCodeNav } from './file-kind';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import BennuI18nPanel from './BennuI18nPanel.svelte';
  import { isI18nBundle } from './i18n/bundle-path';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { bennuCargoStore } from '$lib/stores/bennu/cargo.svelte';
  import { emptyInvocation as emptyCargoInvocation } from '$lib/ipc/bennu/cargo';
  import { bennuRunConfigStore } from '$lib/stores/bennu/run-config.svelte';
  import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
  // Opening a stack frame's source — the same resolution the consoles' stack traces use, so a
  // frame in a dependency lands in its source view rather than nowhere.
  import { openLogLink } from './log-link';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { bennuTestStore } from '$lib/stores/bennu/tests.svelte';
  import { bennuCargoTestStore } from '$lib/stores/bennu/cargo-tests.svelte';
  import { activeTestStore } from '$lib/stores/bennu/test-runner.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import { lspReloadWorkspace } from '$lib/ipc/bennu/lsp';
  import { decompiledStore } from '$lib/stores/bennu/decompiled.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
  import { bennuHierarchyStore } from '$lib/stores/bennu/hierarchy.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { bennuTomcatStore } from '$lib/stores/bennu/tomcat.svelte';
  import { springStore } from '$lib/stores/bennu/spring.svelte';
  import { availableCatalogs } from './framework-catalogs';
  import { hotswapJsp } from '$lib/ipc/bennu/tomcat';
  import { discoverTests } from '$lib/ipc/bennu/tests';
  import { discoverCargoTests } from '$lib/ipc/bennu/cargo-tests';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    // Hydrate the config-backed editor toggles (autosave / auto-import) from the persisted config.
    void bennuSettingsStore.loadConfig();
    // The profile list + which one is active, for the titlebar's switcher. Idempotent, so a Bennu
    // that shares its webview with Corvus does not subscribe twice.
    void profileStore.init();
    // Autosave on frame deactivation (the window loses OS focus) — IntelliJ-style. Guarded by the
    // setting; saves whatever has unsaved edits. `blur` on the window fires when you switch apps.
    const onWindowBlur = () => { if (bennuSettingsStore.autosave) void projectStore.saveAllDirty(); };
    window.addEventListener('blur', onWindowBlur);

    // ── External changes ──────────────────────────────────────────────────────────
    // Notice files rewritten outside Bennu — another editor, a `git checkout`, a generator.
    // A clean buffer adopts the new content silently; a dirty one raises a conflict instead
    // of being overwritten by autosave (see `checkExternalChanges`).
    //
    // On `focus` first, because coming back to the window is when an outside edit is most
    // likely to have just happened — and it must land BEFORE any editing restarts the
    // autosave timer. The tick covers the case focus can't: Bennu owns a terminal, so a
    // `git checkout` can change files while the window never loses focus. It only runs while
    // focused, so a backgrounded window costs nothing.
    const EXTERNAL_POLL_MS = 2000;
    const pollExternal = () => { void projectStore.checkExternalChanges(); };
    let externalTimer: ReturnType<typeof setInterval> | undefined;
    const startPolling = () => {
      if (externalTimer === undefined) externalTimer = setInterval(pollExternal, EXTERNAL_POLL_MS);
    };
    const stopPolling = () => {
      if (externalTimer !== undefined) { clearInterval(externalTimer); externalTimer = undefined; }
    };
    const onWindowFocus = () => { pollExternal(); startPolling(); };
    window.addEventListener('focus', onWindowFocus);
    window.addEventListener('blur', stopPolling);
    if (document.hasFocus()) onWindowFocus();
    // Reopen the last workspace (its projects + tabs) where the user left off — no-op on a fresh
    // install / when the BE is absent. Driven by the workspace store (owns the named-workspace
    // set). Kicks off the index build via the effect below.
    void workspacesStore.restore();
    // The last few hundred milliseconds of a session are the ones a debounce is still holding, and
    // closing the window is exactly when they are lost. Tauri AWAITS this handler before it closes,
    // so writing here is the difference between persisting them and not. No veto: whatever happens
    // to the write, the window must still close.
    const closeWin = getCurrentWindow();
    let unlistenClose: (() => void) | undefined;
    void closeWin
      .onCloseRequested(async () => {
        try {
          await workspacesStore.flush();
        } catch { /* the window closes regardless */ }
      })
      .then((un) => { unlistenClose = un; });
    // Hiding counts too: a machine that sleeps or a window sent to the background can be the last
    // thing that happens before the process goes.
    const onHidden = () => { if (document.hidden) void workspacesStore.flush(); };
    document.addEventListener('visibilitychange', onHidden);
    // `bennu-be` is spawned lazily *as* this window opens, so every load above can run before it is
    // routable — and then nothing asked again. This event is the backend saying "now I am": the
    // deterministic answer, where the workspace store's retry loop is only a fallback for a spawn
    // slower than its window. Both loads are no-ops once they have succeeded.
    let unlistenBeUp: (() => void) | undefined;
    void listen('arbor://bennu-be-up', () => {
      void workspacesStore.restore();
      void bennuSettingsStore.loadConfig();
    }).then((un) => { unlistenBeUp = un; });
    // Subscribe to the build/run + index-progress event streams for this window;
    // detach on unmount.
    let detachRun: (() => void) | undefined;
    let detachIndex: (() => void) | undefined;
    let detachSpell: (() => void) | undefined;
    let detachDecompiled: (() => void) | undefined;
    let detachTests: (() => void) | undefined;
    let detachCargoTests: (() => void) | undefined;
    let detachDebug: (() => void) | undefined;
    let detachLsp: (() => void) | undefined;
    void bennuRunStore.attach().then((d) => { detachRun = d; });
    void bennuTestStore.attach().then((d) => { detachTests = d; });
    void bennuCargoTestStore.attach().then((d) => { detachCargoTests = d; });
    // The debugger's three streams: where the session is, where the program stopped, and what
    // the VM made of each breakpoint.
    void bennuDebugStore.attach().then((d) => { detachDebug = d; });
    void bennuIndexStore.attach().then((d) => { detachIndex = d; });
    void bennuSpellStore.attach().then((d) => { detachSpell = d; });
    // Reload a decompiled tab when its dependency sources finish downloading.
    void decompiledStore.attach().then((d) => { detachDecompiled = d; });
    // Language servers: their catalogue (which decides what the editor even offers for a `.rs`
    // file) and their live status. `attach` also hands the backend its event sink, so without it
    // nothing about a server would ever be pushed — including the progress that explains why a
    // freshly-opened Rust project answers nothing for its first half minute.
    void bennuLspStore.attach().then((d) => { detachLsp = d; });
    // Anti-white-flash: reveal this window once the first real frame is painted.
    requestAnimationFrame(() => requestAnimationFrame(() => void signalWindowReady().catch(() => {})));
    return () => {
      window.removeEventListener('blur', onWindowBlur);
      window.removeEventListener('focus', onWindowFocus);
      window.removeEventListener('blur', stopPolling);
      stopPolling();
      detachRun?.(); detachIndex?.(); detachSpell?.(); detachDecompiled?.(); detachTests?.();
      detachCargoTests?.();
      unlistenClose?.();
      unlistenBeUp?.();
      document.removeEventListener('visibilitychange', onHidden);
      detachDebug?.(); detachLsp?.();
      bennuIndexStore.reset();
    };
  });

  // Name the OS window after the open project — what tells two Bennu windows
  // apart in the taskbar, Alt-Tab and the macOS Window menu.
  syncWindowTitle('Bennu', () => projectStore.project?.name, {
    active: () => surfaceStore.hasFocus('bennu'),
  });

  // Feed Canopy's cross-product recents. The demo project is excluded — it isn't
  // somewhere the user can return to.
  $effect(() => {
    const p = projectStore.project;
    if (!p?.root || projectStore.isDemo) return;
    void recordRecentProject('bennu', p.root, p.name).catch(() => {});
  });

  // Canopy asking for a specific project: open it. Once on mount for a request
  // parked before this window existed, then on every later request.
  onMount(() => onOpenIntent('bennu', (path) => { void projectStore.openProject(path); }));

  /**
   * Follow the debugger: whenever the selected frame changes, open its file at its line.
   *
   * ONE place, and it is why the frames list only selects. Every way of arriving at a frame —
   * hitting a breakpoint, stepping into a method, stepping out of it, clicking a row — is the
   * same event as far as "show me where that is" is concerned, and a debugger that stops
   * without opening the code leaves you to find the file yourself every single time.
   *
   * Here rather than in the Frames column because that column only exists while the program is
   * stopped: an effect inside it would be mounted and torn down on every resume, and would fire
   * on the remount rather than on the stop.
   */
  let shownFrame = '';
  $effect(() => {
    const frame = bennuDebugStore.currentFrame;
    if (!frame) {
      shownFrame = '';
      return;
    }
    // Keyed by what would make it a different *place*, so a re-render or a variables refresh
    // does not re-open (and re-scroll) the file you are already reading.
    const key = `${bennuDebugStore.sessionId}:${frame.index}:${frame.class}:${frame.line ?? ''}`;
    if (key === shownFrame) return;
    shownFrame = key;
    // A project frame carries its file; a library one is resolved on the way — the same path
    // the console's stack traces take, so a step into the JDK lands in its source view.
    void openLogLink(
      frame.file
        ? { kind: 'file', path: frame.file, line: frame.line ?? undefined }
        : { kind: 'source', class: frame.class, method: frame.method, line: frame.line ?? undefined },
    );
  });

  // When a real (non-demo) **Java** project opens, kick off the indexing status + job. The BE
  // rebuilds the index on every open, so this fires each time the root changes.
  //
  // Not for a Cargo project: `bennu_open_project` returns without starting an index there (no
  // Java sources, no classpath), so arming the status here put up an "Indexing project" job
  // that watched a build nobody had started — visible work with nothing behind it, on every
  // single open. The index is a Java-model thing; Rust needs a language server, not this.
  let lastIndexedRoot: string | null = null;
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    if (!root || projectStore.isDemo || projectStore.isCargo) return;
    if (root !== lastIndexedRoot) {
      lastIndexedRoot = root;
      bennuIndexStore.onProjectOpen(root);
      // The test tree and its results belong to the project that produced them. Carrying
      // them into the next one would show green rows for classes this project doesn't have.
      untrack(() => bennuTestStore.reset());
      untrack(() => bennuCargoTestStore.reset());
    }
  });

  // The Cargo workspace, on opening a Rust project. Read here rather than only by the Cargo panel
  // because three other surfaces want it before that panel is ever opened: the run-configuration
  // editor's crate and target pickers, ▶ looking for the sole binary, and the palette. Cheap — it
  // reads manifests, never `cargo metadata`.
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    if (root && projectStore.isCargo) {
      void bennuCargoStore.load(root);
    } else {
      bennuCargoStore.reset();
    }
  });

  // Project-level diagnostics (JDK status + wrong-encoding files) for the titlebar badge +
  // the Problems panel. Re-fetch when the project changes or the index (re)builds — the
  // encoding report lands after the project phase, `buildRevision` catches each phase.
  // Java-only for the same reason: a Cargo project has no JDK to report on, and its encoding
  // is UTF-8 by language definition.
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    void bennuIndexStore.buildRevision; // re-run as the (re)build progresses
    if (root && !projectStore.isDemo && !projectStore.isCargo) {
      void bennuDiagnosticsStore.refresh(root);
    } else {
      bennuDiagnosticsStore.reset();
    }
  });

  // ── Build / Run triggers (mirror the titlebar; shared by keybindings + palette) ─
  /** Ctrl+F9 / palette Build — runs the preferred build type (Maven compile or validation). */
  function triggerBuild() {
    const root = projectStore.project?.root;
    if (root) void bennuRunStore.runPreferred(root);
  }
  /** Palette "Validate project" — the whole-project validation without compiling. */
  function triggerValidate() {
    const root = projectStore.project?.root;
    if (root) void bennuRunStore.validateProject(root);
  }
  function triggerRun() {
    const root = projectStore.project?.root;
    if (!root) return;
    // Honour the ACTIVE run configuration — all of it, VM args and environment included.
    // With none, the store looks for the project's entry points and runs the one it finds;
    // the editor opens only when there is a real question to answer (several, or none).
    void bennuRunStore.runActive(root).then((ran) => { if (!ran) bennuUiStore.openRunConfig(); });
  }
  /**
   * Shift+F9 / palette Debug — the same launch, under the debugger.
   *
   * The panel opens with it, because a debug launch you cannot see the state of is just a
   * slower run: whether it attached, whether the breakpoints took, and where it stopped are
   * all in there.
   */
  function triggerDebug() {
    const root = projectStore.project?.root;
    if (!root) return;
    bennuUiStore.showBottom('run');
    void bennuRunStore.runActive(root, true).then((ran) => {
      if (!ran) bennuUiStore.openRunConfig();
    });
  }
  /**
   * Ctrl+F8 — set or clear a breakpoint on the caret's line, without reaching for the gutter.
   *
   * The editor answers, because *where a breakpoint may go* is a property of the buffer: only a
   * line that compiles to bytecode can hold one, and asking the store directly would be a
   * second answer to a question the gutter already answers. `false` means the caret is not on
   * such a line — the key then does nothing, exactly as the gutter offers nothing there.
   */
  function toggleBreakpointAtCaret() {
    editor?.toggleBreakpointAtCaret();
  }

  // ── Tests ─────────────────────────────────────────────────────────────────────
  /** Run every test in the project (Ctrl+Shift+F5 / palette). */
  function triggerRunAllTests() {
    const root = projectStore.project?.root;
    if (root) void activeTestStore().runAll(root);
  }

  /**
   * Run the Rust test at the caret.
   *
   * The same rule as the Java path — the last declaration at or above the caret — with cargo's
   * extra fact: an `#[rstest]` produces several libtest cases named after it, so it is asked for
   * by prefix rather than exactly (`caseRefOf` decides). With the caret above every test, the
   * whole file's target runs, which is the nearest thing cargo can express to "this file".
   */
  async function runRustTestAtCaret(root: string, file: string, line: number) {
    const tests = await discoverCargoTests(root, { file }).catch(() => []);
    if (!tests.length) {
      toastStore.show('This file declares no tests', 'info');
      return;
    }
    const owner = tests.filter((t) => t.line <= line).sort((a, b) => a.line - b.line).pop();
    if (owner) {
      void bennuCargoTestStore.runCases(root, [bennuCargoTestStore.caseRefOf(owner)]);
      return;
    }
    // Above the first test: run the target the file belongs to.
    const first = tests[0];
    void bennuCargoTestStore.run(root, {
      kind: 'target',
      package: first.package,
      target: first.target,
    });
  }

  /**
   * Run the test **at the caret** (Ctrl+Shift+F10 — IntelliJ's "run context configuration").
   *
   * Resolves against a fresh scan of the file on disk rather than the cached project-wide
   * discovery: this is the one path where being one save out of date means running the wrong
   * thing. The method chosen is the last one declared at or above the caret — which is what
   * "the test I am inside" means, since a method's body follows its signature. With the
   * caret above the first test (in the imports, in a field) there is no method to mean, so
   * the whole class runs.
   */
  async function triggerRunTestAtCaret() {
    const root = projectStore.project?.root;
    const file = projectStore.activeFilePath;
    if (!root || !file) return;
    const line = editor?.getCaretLine() ?? 1;
    if (projectStore.isCargo) {
      await runRustTestAtCaret(root, file, line);
      return;
    }
    const classes = await discoverTests(root, { file }).catch(() => []);
    // A file may declare several test classes (a nested `@Nested`, a helper beside the main
    // one); the one that owns the caret is the last one that starts at or above it.
    const owner = classes
      .filter((c) => c.line <= line && !c.is_abstract)
      .sort((a, b) => a.line - b.line)
      .pop();
    if (!owner) {
      if (classes.length) toastStore.show('No runnable test class at the caret', 'info');
      else toastStore.show('This file declares no tests', 'info');
      return;
    }
    const method = owner.methods
      .filter((m) => m.line <= line)
      .sort((a, b) => a.line - b.line)
      .pop();
    if (method) void bennuTestStore.runCase(root, owner.selector, method.name);
    else void bennuTestStore.runClass(root, owner.selector);
  }

  /** Whether the active file declares any test — gates the caret-run verb + its shortcut, so
   *  the key is silent on a file where it could only ever say "nothing here". */
  const activeFileHasTests = $derived.by(() => {
    const file = projectStore.activeFilePath;
    if (!file) return false;
    return projectStore.isCargo
      ? bennuCargoTestStore.testsInFile(file).length > 0
      : bennuTestStore.classesInFile(file).length > 0;
  });

  let editor = $state<{
    openGoto: () => void;
    expandSelection: () => Promise<boolean>;
    shrinkSelection: () => Promise<boolean>;
    getCaretLine: () => number;
    openSearch: () => void;
    focusEditor: () => void;
    openIntentions: () => void;
    goToDefinition: () => void;
    openRename: () => void;
    findUsages: () => void;
    showCallHierarchy: () => void;
    showTypeHierarchy: () => void;
    expandMacro: () => void;
    insertAtCursor: (text: string) => void;
    getSelectedText: () => string;
    checkMojibake: () => void;
    createValidationFile: () => void;
    toggleBreakpointAtCaret: () => void;
    formatDocument: () => Promise<void>;
    navBack: () => void;
    navForward: () => void;
  } | null>(null);

  /**
   * Rebuild the language server's model of the project — re-read the manifests, re-resolve the crate
   * graph.
   *
   * rust-analyzer reloads on its own when a `Cargo.toml` it knows about changes, so this is for the
   * cases it cannot see: a `.cargo/config.toml` edit, a patched or vendored dependency changing
   * underneath, a `cargo add` run in a terminal, a git dependency that has moved. Which is also why
   * it is a command and not something Bennu fires after every manifest save — a second
   * `cargo metadata` plus a build-script rebuild is seconds of work for an answer the server already
   * had.
   */
  async function reloadLanguageServerWorkspace() {
    const scope = projectStore.project?.root;
    if (!scope) return;
    const ok = await lspReloadWorkspace(scope).catch(() => false);
    toastStore.show(
      ok ? 'Reloading the workspace…' : 'No language server to reload for this project',
      ok ? 'info' : 'warning',
    );
  }

  /** Restart the language server serving the open file — the way out of a failed slot (failures
   *  are sticky on purpose) and the fix for "I just installed it". */
  async function restartActiveLanguageServer() {
    const status = bennuLspStore.statusFor(projectStore.activeFilePath);
    if (!status) { toastStore.show('No language server for this file', 'info'); return; }
    await bennuLspStore.restart(status.root, status.language);
    toastStore.show(`Restarting ${status.name}…`, 'info');
  }

  /** Ctrl+S — save the active file to disk. */
  function saveActive() {
    void projectStore.saveActive().then((ok) => { if (ok) toastStore.show('Saved', 'success'); });
  }

  // Alt+Enter "Generate…" intention → open the Generate modal in that mode.
  function openGenerateFromIntention(mode: GenerateMode) {
    bennuUiStore.openGenerate(mode);
  }

  // ── Tomcat JSP hot-swap ───────────────────────────────────────────────────────
  /** Deploy the current JSP (`all=false`) or every project JSP (`all=true`) into the linked
   *  Tomcat's exploded webapp. Opens the Tomcat settings modal when no server is linked yet.
   *  The BE fires the success/failure toast; the current buffer is saved first so fresh bytes ship. */
  async function deployToTomcat(all: boolean) {
    const root = projectStore.project?.root;
    if (!root) return;
    if (!bennuTomcatStore.isLoaded(root)) await bennuTomcatStore.load(root);
    if (!bennuTomcatStore.isLinked(root)) {
      toastStore.show('Link a Tomcat to hot-swap JSPs', 'info');
      bennuUiStore.openTomcatConfig();
      return;
    }
    const path = projectStore.activeFilePath;
    if (!all && !isJspFile(path)) { toastStore.show('Open a JSP to deploy it', 'info'); return; }
    // Save the active JSP first so the copy ships the latest edits (byte-for-byte off disk).
    if (!all) await projectStore.saveActive();
    try {
      await hotswapJsp(root, all ? undefined : path ?? undefined);
      // The BE emits the success toast (with the count / target); nothing else to do here.
    } catch {
      // The BE also emits the failure toast; swallow to avoid a duplicate.
    }
  }

  // ── Left/right rail items ────────────────────────────────────────────────────
  //
  // Java-only tools are absent — not disabled — on a Cargo project. Structure, Maven,
  // Dependencies and Forms are each backed by the Java symbol index or by the
  // Struts/Spring config graph, and neither exists for a Rust root (see
  // `bennu_open_project`): a rail icon that always opens an empty panel teaches the wrong
  // thing about the tool. Project, Build, Problems, TODO and Terminal all mean something
  // for both and stay.
  const javaTools = $derived(!projectStore.isCargo);
  // Forms reads a JSP's `<form>`s across the include graph, so a project with no pages —
  // a service module, a library, a Cargo root — has nothing it could ever show. Same reasoning
  // as `javaTools`, one notch narrower: the capability set says whether pages exist at all.
  const jspTools = $derived(javaTools && projectStore.capabilities?.jsp_views === true);
  // The Struts `*-validation.xml` tooling — meaningless on a project that doesn't use Struts.
  const hasStruts = $derived(
    projectStore.capabilities?.struts_xml_config === true
      || projectStore.capabilities?.struts_convention === true,
  );
  // Framework tooling (Beans / Endpoints / Config / JPA): which panels this project gets.
  //
  // Answered by the backend rather than re-derived from the capability bitset here — it owns both
  // the capability gate and the models built behind it, so this is one round-trip per project.
  //
  // A notch narrower than `javaTools`, for the same reason `jspTools` is: an extension applying
  // says the tooling is *relevant*, its counts say whether any of it found anything. A Spring
  // service with no REST controllers should not carry an Endpoints button that can only ever open
  // an empty list — and the rail is the one piece of chrome on screen all the time, so it is the
  // worst place to keep a promise the project can't fulfil.
  //
  // This is also why "does any extension apply" is not the gate it once was: XML applies to every
  // project, so that question has answered yes for everything since the XML extension landed.
  // Not gated on `javaTools`: which catalogs a project gets is decided by what the extensions
  // actually found (`availableCatalogs` filters on the counts), and gating on the ecosystem was
  // what hid the fulcrum i18n one on the projects that have it.
  const catalogs = $derived(availableCatalogs(springStore.stats));
  const catalogIds = $derived(catalogs.map((c) => c.id));
  // Every JPA generator writes into an existing entity or repository, so the verbs are worth
  // offering exactly when the project has some.
  const hasJpa = $derived(
    catalogIds.includes('jpaentities') || catalogIds.includes('jparepositories'),
  );
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    // Re-read when the index **stops**: that is exactly when new beans, new endpoints and new
    // property files appear.
    //
    // NOT on `buildRevision`, which ticks on every index-progress event — including the per-file
    // ones of the reference walk. Depending on that meant one `bennu_ext_overview` per file
    // indexed: thousands of requests on a real project, each on its own backend thread, and the
    // backend stops answering anything at all. The log for it reads as unanswered calls piling
    // up across every unrelated domain, which is what makes it hard to attribute.
    const busyIndexing = bennuIndexStore.indexing;
    if (!root || projectStore.isDemo) {
      springStore.reset();
      return;
    }
    // A Cargo root asks too, and does not wait: the framework seam is no longer Java's alone — the
    // fulcrum i18n extension applies to a project whose only Java is none — and there is no Java
    // semantic index building on one, so there is nothing to race. A project the extensions all
    // decline costs one capability check and no walk at all.
    if (!projectStore.isCargo && busyIndexing) return;
    void springStore.loadOverview(root, true);
  });

  /**
   * Load the project's test classes once the index has settled.
   *
   * At the window level rather than inside the Tests panel, because the tree's "Run tests
   * in…" entry has to know whether a folder contains any *before* it is clicked — and the
   * panel may never have been opened. Deferred until indexing stops for the same reason the
   * framework overview is: the walk parses every source file in the tree, and racing it against
   * the indexer buys nothing.
   *
   * A Cargo workspace does discover too, and does not wait: the Java semantic index is not
   * building on one, so there is nothing to race.
   */
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    const cargo = projectStore.isCargo;
    const busyIndexing = bennuIndexStore.indexing;
    if (!root || projectStore.isDemo || (!cargo && busyIndexing)) return;
    void activeTestStore().discover(root);
  });

  /**
   * Hydrate the project's run configurations from its `.arbor/bennu/config.toml`. Cheap (one small
   * file) and needed before the titlebar ▶ can mean anything, so it does not wait for the
   * index the way test discovery does. The store makes it idempotent per root — this effect
   * re-runs on more than "a different project", and a re-read would discard edits.
   */
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    if (!root || projectStore.isDemo) return;
    void bennuRunConfigStore.load(root);
    // The breakpoints live beside them, in the same file and for the same reason: the gutter
    // has to draw them the moment a file opens, long before anything is launched.
    void bennuDebugStore.load(root);
  });

  /**
   * Drop the hierarchy when the project changes.
   *
   * The tree is an answer about one file of one project, and its nodes are handles a *particular*
   * language server issued. After a switch those handles belong to a session that no longer answers
   * for anything on screen — so the tree would look current and expand into nothing.
   */
  let hierarchyRoot: string | null = null;
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    if (root === hierarchyRoot) return;
    hierarchyRoot = root;
    // `untrack`: clearing touches store state this effect must not then react to.
    untrack(() => bennuHierarchyStore.clear());
  });

  const leftTop = $derived<ActivityRailItem[]>([
    { id: 'project',   tooltip: 'Project',   shortcut: 'Alt+1', icon: FolderTree, active: bennuUiStore.leftPanel === 'project',   onclick: () => bennuUiStore.toggleLeft('project') },
    ...(javaTools
      ? [
          { id: 'structure', tooltip: 'Structure', shortcut: 'Alt+2', icon: ListTree,   active: bennuUiStore.leftPanel === 'structure', onclick: () => bennuUiStore.toggleLeft('structure') },
        ]
      : []),
    // Both ecosystems: the panel answers for a Maven reactor and for a Cargo workspace, from one
    // report shape.
    { id: 'dependencies', tooltip: 'Dependencies', shortcut: 'Alt+N', icon: Library, active: bennuUiStore.leftPanel === 'dependencies', onclick: () => bennuUiStore.toggleLeft('dependencies') },
  ]);
  // Left rail bottom cluster: only the bottom-dock toggles (Terminal, Problems).
  // Docs/Settings moved to the titlebar's right cluster (IntelliJ/Corvus layout).
  // These drive the BOTTOM dock (BennuBottomDock), not a side panel — the active
  // state mirrors the dock's open tab.
  const leftBottom = $derived<ActivityRailItem[]>([
    { id: 'build',    tooltip: 'Build', shortcut: 'Alt+0',      icon: Hammer,         active: bennuUiStore.bottomPanel === 'build',    onclick: () => bennuUiStore.toggleBottom('build') },
    // ONE button for running and debugging, because they are one activity: the same launch with
    // more to look at. The icon says which it currently is, and the dot is a WARNING while the
    // program is stopped — a suspended VM holds its locks and its port, and a debug session you
    // forgot about looks exactly like a hang.
    //
    // Not Java-only any more: a cargo command streams into the same console through the same
    // registry, so Stop, ⟳ and the tab strip all mean the same thing on a Rust project.
    {
      id: 'run',
      tooltip: bennuDebugStore.live ? 'Run / Debug' : 'Run',
      shortcut: 'Alt+R',
      icon: bennuDebugStore.live ? Bug : Play,
      dot: bennuDebugStore.paused
        ? ('warning' as const)
        : bennuRunStore.running
          ? ('accent' as const)
          : undefined,
      active: bennuUiStore.bottomPanel === 'run',
      onclick: () => bennuUiStore.toggleBottom('run'),
    },
    { id: 'problems', tooltip: 'Problems', shortcut: 'Alt+6',   icon: AlertTriangle,  active: bennuUiStore.bottomPanel === 'problems', onclick: () => bennuUiStore.toggleBottom('problems') },
    { id: 'todos',    tooltip: 'TODO', shortcut: 'Alt+7',       icon: ListTodo,       active: bennuUiStore.bottomPanel === 'todos',    onclick: () => bennuUiStore.toggleBottom('todos') },
    { id: 'terminal', tooltip: 'Terminal', shortcut: 'Alt+F12', icon: TerminalSquare, active: bennuUiStore.bottomPanel === 'terminal', onclick: () => bennuUiStore.toggleBottom('terminal') },
  ]);
  /** The build tool's own window — Maven's goals or Cargo's crates. One slot, because a project is
   *  one or the other, and Alt+8 means "the build tool" either way. */
  const buildToolRail = $derived<ActivityRailItem>(
    projectStore.isCargo
      ? { id: 'cargo', tooltip: 'Cargo — the crates, and what you can run on them', shortcut: 'Alt+8', icon: Cog, active: bennuUiStore.rightPanel === 'cargo', onclick: () => bennuUiStore.toggleRight('cargo') }
      : { id: 'maven', tooltip: 'Maven', shortcut: 'Alt+8', icon: MavenIcon, active: bennuUiStore.rightPanel === 'maven', onclick: () => bennuUiStore.toggleRight('maven') },
  );
  const rightTop = $derived<ActivityRailItem[]>([
    buildToolRail,
    // The CATALOGUE of tests, not the runs — those are tabs of the Run console. Present on both
    // ecosystems: a Cargo workspace enumerates its `#[test]`s exactly as a Maven project does, and
    // the panel behind this button is the per-ecosystem one.
    {
      id: 'tests',
      tooltip: 'Tests',
      shortcut: 'Alt+5',
      icon: projectStore.isCargo ? RustTestIcon : JUnitIcon,
      active: bennuUiStore.rightPanel === 'tests',
      onclick: () => bennuUiStore.toggleRight('tests'),
    },
    ...(javaTools
      ? [
          // The parse, and the model Bennu derives from it. Both views are about Bennu's OWN
          // engines — the tree-sitter grammars are Java and JSP, and the model is Java's — so on a
          // Cargo root the panel can only ever say "no grammar for Rust". It used to be offered
          // there on the grounds that naming a language it cannot read is an honest answer, and
          // that stopped being true the moment rust-analyzer started serving those files richly:
          // the message then reads as "Bennu does not understand Rust", which is the opposite of
          // the case. An absence is only worth reporting when nothing else is answering.
          { id: 'ast', tooltip: 'Trees — the parse, and the model Bennu derives from it', shortcut: 'Alt+9', icon: Braces, active: bennuUiStore.rightPanel === 'ast', onclick: () => bennuUiStore.toggleRight('ast') },
        ]
      : []),
  ]);
  // Forms drives the BOTTOM dock (wide, horizontal data), not a side panel — its toggle sits
  // in the right rail's bottom cluster; the active state mirrors the dock's open tab.
  const rightBottom = $derived<ActivityRailItem[]>([
    ...(jspTools
      ? [{ id: 'forms', tooltip: 'Forms', shortcut: 'Alt+3', icon: TextCursorInput, active: bennuUiStore.bottomPanel === 'forms', onclick: () => bennuUiStore.toggleBottom('forms') }]
      : []),
    // The framework catalogs that asked for a rail button — today just Endpoints, which is a
    // list you keep open while working rather than one you go and fetch. The rest stay
    // palette-only so the rail doesn't grow a row per framework. `catalogs` has already dropped
    // the ones this project found nothing for.
    ...catalogs
      .filter((c) => c.rail)
      .map((c) => ({
        id: c.id,
        tooltip: c.title,
        shortcut: c.shortcut,
        icon: Target,
        active: bennuUiStore.bottomPanel === c.id,
        onclick: () => bennuUiStore.toggleBottom(c.id),
      })),
  ]);

  // Switching projects can take rail icons away — a Cargo root loses the Java tools, a
  // project with no JSP loses Forms, a project whose Spring model has no routes loses Endpoints.
  // A panel left open from the previous project would then have no toggle anywhere. Close those.
  $effect(() => {
    const java = javaTools;
    const jsp = jspTools;
    const available = catalogIds;
    // `untrack`: the call reads the very panel state it writes, and an effect that depends
    // on what it assigns is the shape that loops (CLAUDE.md · "Runes — trap da evitare").
    // The only dependencies that should re-run this are the three values read above.
    untrack(() =>
      bennuUiStore.dropUnavailablePanels({
        left: java ? ['project', 'structure', 'dependencies'] : ['project', 'dependencies'],
        // `i18n` is in both branches and needs no capability gate: it has no rail icon to lose, and
        // whether it applies is a question about the open FILE rather than about the project — a
        // fulcrum content tree can sit in a Maven repo as easily as in a Cargo one.
        right: java ? ['maven', 'tests', 'ast', 'i18n'] : ['cargo', 'i18n'],
        bottom: [
          // `hierarchy` survives every switch: it is opened by an action about the caret rather than
          // by a rail button, so there is no icon that could disappear from under it, and its own
          // header closes it.
          'problems', 'terminal', 'build', 'todos', 'run', 'hierarchy',
          ...(jsp ? ['forms' as const] : []),
          // Most framework catalogs have no rail button, so one left open after switching to a
          // project that doesn't offer it would be unclosable from the rail.
          ...available,
        ],
      }),
    );
  });

  const showLeft   = $derived(bennuUiStore.leftPanel !== null);
  const showRight  = $derived(bennuUiStore.rightPanel !== null);
  /** Whether the open right panel wants the wider column — see the card below. */
  const wideRight  = $derived(bennuUiStore.rightPanel === 'i18n');
  const showBottom = $derived(bennuUiStore.bottomPanel !== null);
  // A job's output was opened from the (shared) Jobs overlay — the shared uiStore drives it, exactly
  // like corvus. It takes the bottom slot while shown; closing it (its back/close button clears the
  // shared section) falls back to Bennu's own bottom dock.
  const showJobOutput = $derived(
    sharedUiStore.activeBottomSection === 'jobs' && jobsStore.activeJobId !== null,
  );

  // ── Command palette ────────────────────────────────────────────────────────
  let paletteQuery = $state('');

  // Reset the query every time the palette OPENS, so reopening never re-shows the
  // previous command's text (the query survives a close otherwise — it's `bind:`-ed).
  let paletteWasOpen = false;
  $effect(() => {
    const open = bennuUiStore.paletteOpen;
    if (open && !paletteWasOpen) paletteQuery = '';
    paletteWasOpen = open;
  });

  const ICONS: Record<string, IconComponent> = {
    'folder-tree': FolderTree as unknown as IconComponent,
    'list-tree': ListTree as unknown as IconComponent,
    'list': TextCursorInput as unknown as IconComponent,
    'library': Library as unknown as IconComponent,
    'search': Search as unknown as IconComponent,
    'hash': Hash as unknown as IconComponent,
    'target': Target as unknown as IconComponent,
    'file': FileCode2 as unknown as IconComponent,
    'alert': AlertTriangle as unknown as IconComponent,
    'terminal': TerminalSquare as unknown as IconComponent,
    'hammer': Hammer as unknown as IconComponent,
    'maven': MavenIcon as unknown as IconComponent,
    'cog': Cog as unknown as IconComponent,
    'junit': JUnitIcon as unknown as IconComponent,
    'braces': Braces as unknown as IconComponent,
    'list-checks': ListChecks as unknown as IconComponent,
    'play': Play as unknown as IconComponent,
    'bug': Bug as unknown as IconComponent,
    'flask': FlaskConical as unknown as IconComponent,
    'rerun': ListRestart as unknown as IconComponent,
    'todo': ListTodo as unknown as IconComponent,
    'box': Box as unknown as IconComponent,
    'server': Server as unknown as IconComponent,
    'command': Command as unknown as IconComponent,
    'wand': Wand2 as unknown as IconComponent,
    'bulb': Lightbulb as unknown as IconComponent,
    'sliders': SlidersHorizontal as unknown as IconComponent,
    'info': Info as unknown as IconComponent,
    'refresh-cw': RotateCw as unknown as IconComponent,
    'indent': IndentIncrease as unknown as IconComponent,
    'shield': ShieldCheck as unknown as IconComponent,
    // The two framework catalogs that were falling through to the generic `command` glyph:
    // a bound-properties list and the property reference read out of the dependency jars.
    // (`list` is declared once, above — a second entry here silently shadowed it.)
    'book': BookOpen as unknown as IconComponent,
    'languages': Languages as unknown as IconComponent,
    'network': Network as unknown as IconComponent,
  };
  function iconResolver(name: string): IconComponent { return ICONS[name] ?? ICONS.command; }

  function run(fn: () => void) { bennuUiStore.closePalette(); queueMicrotask(fn); }

  const paletteSections = $derived.by<PaletteSection[]>(() => {
    const q = paletteQuery.trim().toLowerCase();
    // File-kind gates — hide actions that don't apply to the open file (no Generate /
    // Java intentions off a `.java`, no go-to / rename / usages off a navigable file).
    const path = projectStore.activeFilePath;
    const canNav = supportsCodeNav(path);
    const isJava = isJavaFile(path);
    const editorItems = [
      { id: 'goto', title: 'Go to line', icon: 'hash', shortcut: 'Ctrl+G',
        action: () => run(() => editor?.openGoto()), when: !!projectStore.activeFilePath },
      { id: 'gotodef', title: 'Go to declaration', icon: 'target', shortcut: 'Ctrl+B',
        action: () => run(() => editor?.goToDefinition()), when: canNav },
      // Not gated on the ecosystem: the navigator has two engines behind it. A Java project's
      // types and members come from the symbol index; a Cargo project's come from the language
      // server, and the tab reads **Types** there because what it finds are structs, enums and
      // traits. Which engine answers is the modal's business — see `BennuGotoModal.lspBacked`.
      { id: 'gotoclass', title: javaTools ? 'Go to class…' : 'Go to type…', icon: 'box', shortcut: 'Ctrl+N',
        action: () => run(() => bennuUiStore.openNav('class', editor?.getSelectedText() ?? '')),
        when: !!projectStore.project },
      { id: 'gotofile', title: 'Go to file…', icon: 'file', shortcut: 'Ctrl+Shift+N',
        action: () => run(() => bennuUiStore.openNav('file', editor?.getSelectedText() ?? '')), when: !!projectStore.project },
      { id: 'gotosymbol', title: 'Go to symbol…', icon: 'search', shortcut: 'Ctrl+Shift+Y',
        action: () => run(() => bennuUiStore.openNav('symbol', editor?.getSelectedText() ?? '')),
        when: !!projectStore.project },
      { id: 'expandsel', title: 'Expand selection', icon: 'braces', shortcut: 'Alt+Shift+Right',
        action: () => run(() => void editor?.expandSelection()), when: canNav },
      { id: 'shrinksel', title: 'Shrink selection', icon: 'braces', shortcut: 'Alt+Shift+Left',
        action: () => run(() => void editor?.shrinkSelection()), when: canNav },
      { id: 'filestructure', title: 'File structure…', icon: 'list-tree', shortcut: 'Ctrl+F12',
        action: () => run(() => bennuUiStore.openFileStructure()), when: canNav },
      { id: 'usages', title: 'Find usages', icon: 'search', shortcut: 'Alt+F7',
        action: () => run(() => void editor?.findUsages()), when: canNav },
      // Only for a server-backed buffer: the Java engine has find-usages and no hierarchy, so on a
      // `.java` these verbs would open a panel that can only ever say "nothing here".
      { id: 'callhierarchy', title: 'Call hierarchy', icon: 'network', shortcut: 'Ctrl+Shift+H',
        action: () => run(() => editor?.showCallHierarchy()), when: isLspFile(path) },
      { id: 'typehierarchy', title: 'Type hierarchy', icon: 'network', shortcut: 'Ctrl+H',
        action: () => run(() => editor?.showTypeHierarchy()), when: isLspFile(path) },
      { id: 'rename', title: 'Rename…', icon: 'target', shortcut: 'Shift+F6',
        action: () => run(() => editor?.openRename()), when: canNav },
      { id: 'save', title: 'Save file', icon: 'file', shortcut: 'Ctrl+S',
        action: () => run(saveActive), when: !!projectStore.activeFilePath },
      { id: 'find', title: 'Find in file', icon: 'search', shortcut: 'Ctrl+F',
        action: () => run(() => editor?.openSearch()), when: !!projectStore.activeFilePath },
      { id: 'findproj', title: 'Find in project', icon: 'search', shortcut: 'Ctrl+Shift+F',
        action: () => run(() => bennuUiStore.openFind(editor?.getSelectedText() ?? '')), when: true },
      { id: 'reveal', title: 'Select opened file in tree', icon: 'folder-tree',
        action: () => run(() => bennuUiStore.revealActiveInTree()), when: !!projectStore.activeFilePath },
      { id: 'generate', title: 'Generate…', icon: 'wand', shortcut: 'Alt+Insert',
        action: () => run(() => bennuUiStore.openGenerate()), when: isJava },
      { id: 'intentions', title: 'Show intentions', icon: 'bulb', shortcut: 'Alt+Enter',
        // Also the language-server quick-fix list for a server-backed buffer — the user's gesture
        // is "what can you do here", and which engine answers is not their problem.
        action: () => run(() => editor?.openIntentions()), when: isJava || isLspFile(path) },
      // NOT IntelliJ's Ctrl+Alt+L: on IT/DE/FR/ES layouts Chromium drops Ctrl+Alt+<letter> to
      // preserve AltGr, so the binding would simply never fire. Alt+Shift+F is VS Code's and is
      // in the safe family.
      { id: 'format', title: 'Format file', icon: 'wand', shortcut: 'Alt+Shift+F',
        action: () => run(() => void editor?.formatDocument()), when: isLspFile(path) },
      // What a macro expands to. Recursive — the server has no single-step form — and read-only,
      // because what comes back is text rather than a file it knows.
      { id: 'expandmacro', title: 'Expand macro', icon: 'wand', shortcut: 'Alt+Shift+M',
        action: () => run(() => editor?.expandMacro()), when: isLspFile(path) },
      { id: 'lsp-restart', title: 'Restart language server', icon: 'refresh-cw',
        action: () => run(() => void restartActiveLanguageServer()),
        when: !!bennuLspStore.statusFor(path) },
      // Not the same thing as a restart: this re-reads the manifests in the SAME session, keeping
      // everything the server has already indexed.
      { id: 'lsp-reload', title: 'Reload workspace (re-read the manifests)', icon: 'refresh-cw',
        action: () => run(() => void reloadLanguageServerWorkspace()),
        when: !!projectStore.project && !!bennuLspStore.statusFor(path) },
      { id: 'lsp-settings', title: 'Language server settings…', icon: 'sliders',
        action: () => run(() => bennuUiStore.openSettings('languages')), when: true },
      { id: 'mojibake', title: 'Check file for mojibake', icon: 'shield',
        action: () => run(() => void editor?.checkMojibake()), when: !!path },
      { id: 'mojibakeproject', title: 'Scan project for mojibake…', icon: 'shield',
        action: () => run(() => bennuUiStore.openMojibakeScan()), when: !!projectStore.project },
      { id: 'newvalidator', title: 'Add Struts validators…', icon: 'shield',
        action: () => run(() => bennuUiStore.openValidationCreator()),
        when: projectStore.activeFilePath?.toLowerCase().endsWith('-validation.xml') ?? false },
      { id: 'createvalidation', title: 'Create Struts validation file', icon: 'shield',
        action: () => run(() => void editor?.createValidationFile()), when: isJava && hasStruts },
      { id: 'hotswap-jsp', title: 'Deploy current JSP to Tomcat', icon: 'server', shortcut: 'Ctrl+Shift+F10',
        action: () => run(() => void deployToTomcat(false)), when: isJspFile(path) },
      // Indentation — mirrors the footer control (BennuIndentStatus). Gated to the
      // alternatives only (the active style / width is hidden), so at most 3 entries show.
      { id: 'indent-spaces', title: 'Indent using spaces', icon: 'indent',
        action: () => run(() => bennuSettingsStore.setIndentStyle('spaces')),
        when: bennuSettingsStore.indentStyle !== 'spaces' },
      { id: 'indent-tabs', title: 'Indent using tabs', icon: 'indent',
        action: () => run(() => bennuSettingsStore.setIndentStyle('tabs')),
        when: bennuSettingsStore.indentStyle !== 'tabs' },
      { id: 'tabwidth-2', title: 'Tab width: 2', icon: 'indent',
        action: () => run(() => bennuSettingsStore.setTabSize(2)), when: bennuSettingsStore.tabSize !== 2 },
      { id: 'tabwidth-4', title: 'Tab width: 4', icon: 'indent',
        action: () => run(() => bennuSettingsStore.setTabSize(4)), when: bennuSettingsStore.tabSize !== 4 },
      { id: 'tabwidth-8', title: 'Tab width: 8', icon: 'indent',
        action: () => run(() => bennuSettingsStore.setTabSize(8)), when: bennuSettingsStore.tabSize !== 8 },
    ];
    const viewItems = [
      { id: 'project',   title: 'Toggle Project',   icon: 'folder-tree', shortcut: 'Alt+1', action: () => run(() => bennuUiStore.toggleLeft('project')), when: true },
      // The Java-only tools are gated on `javaTools`, exactly like their rail icons — a
      // palette entry that opens a permanently-empty panel is the same lie in a different
      // place.
      { id: 'structure', title: 'Toggle Structure', icon: 'list-tree',   shortcut: 'Alt+2', action: () => run(() => bennuUiStore.toggleLeft('structure')), when: javaTools },
      { id: 'forms',     title: 'Toggle Forms',     icon: 'list',        shortcut: 'Alt+3', action: () => run(() => bennuUiStore.toggleBottom('forms')), when: jspTools },
      { id: 'dependencies', title: 'Dependencies',  icon: 'library',     shortcut: 'Alt+N', action: () => run(() => bennuUiStore.toggleLeft('dependencies')), when: true },
      // The same subject from the other angle: the list says what each module needs, the graph says
      // who needs *it*, what a change to it rebuilds, and whether the project has a cycle. Named for
      // the ecosystem's own word so a Rust workspace is not offered a "module" graph.
      { id: 'modulegraph', title: projectStore.isCargo ? 'Crate graph' : 'Module graph', icon: 'network',
        shortcut: 'Alt+Shift+D', action: () => run(() => bennuUiStore.openModuleGraph()), when: !!projectStore.project },
      // Offered only on a translation bundle: everywhere else the panel could only say "not here",
      // and a palette that lists what cannot work is a palette you stop trusting.
      { id: 'i18npanel', title: 'Toggle i18n panel', icon: 'languages', shortcut: 'Alt+Shift+I',
        action: () => run(() => bennuUiStore.toggleRight('i18n')), when: isI18nBundle(path) },
      { id: 'runpanel',  title: 'Toggle Run',       icon: 'play',        shortcut: 'Alt+R', action: () => run(() => bennuUiStore.toggleBottom('run')), when: true },
      { id: 'tests',     title: 'Toggle Tests',     icon: 'junit',       shortcut: 'Alt+5', action: () => run(() => bennuUiStore.toggleRight('tests')), when: javaTools },
      { id: 'problems',  title: 'Toggle Problems',  icon: 'alert',       shortcut: 'Alt+6', action: () => run(() => bennuUiStore.toggleBottom('problems')), when: true },
      { id: 'todos',     title: 'Toggle TODO',      icon: 'todo',        shortcut: 'Alt+7', action: () => run(() => bennuUiStore.toggleBottom('todos')), when: true },
      { id: 'terminal',  title: 'Toggle Terminal',  icon: 'terminal',    shortcut: 'Alt+F12', action: () => run(() => bennuUiStore.toggleBottom('terminal')), when: true },
      { id: 'maven',     title: 'Toggle Maven',     icon: 'maven',       shortcut: 'Alt+8', action: () => run(() => bennuUiStore.toggleRight('maven')), when: javaTools },
      { id: 'cargo',     title: 'Toggle Cargo',     icon: 'cog',         shortcut: 'Alt+8', action: () => run(() => bennuUiStore.toggleRight('cargo')), when: projectStore.isCargo },
      // Runs the real `cargo add`. In the View section beside the Cargo window because that is where
      // it is otherwise reached from, and gated on the ecosystem: there is nothing to add to a pom.
      { id: 'cargoadd',  title: 'Add dependency… (cargo add)', icon: 'library', action: () => run(() => bennuUiStore.openCargoAdd()), when: projectStore.isCargo },
      { id: 'ast',       title: 'Toggle Trees — syntax and model', icon: 'braces',    shortcut: 'Alt+9', action: () => run(() => bennuUiStore.toggleRight('ast')), when: javaTools },
      { id: 'ssr',       title: 'Structural search / replace…', icon: 'search', shortcut: 'Ctrl+Shift+M', action: () => run(() => bennuUiStore.openSsr()), when: javaTools },
      // The framework catalogs. Palette-only by design (see `framework-catalogs.ts`) and gated
      // on the project having something in them, so they are absent — not empty — everywhere
      // else. Same list the rail is built from: a verb the palette offers must have somewhere to
      // go, and a panel the palette can open must be closable from the rail.
      ...catalogs.map((c) => ({
        id: `cat:${c.id}`,
        title: c.command,
        icon: c.icon,
        shortcut: c.shortcut as string | undefined,
        action: () => run(() => bennuUiStore.toggleBottom(c.id)),
        when: true,
      })),
      // JPA generation. Gated on the project having entities or repositories — on a project with
      // no persistence the verb is absent rather than opening a form with nothing to build against.
      // One entry per generator, so every one is reachable by name from the keyboard. The
      // toolbar's list is per-file and comes from the backend; this one is per-project and
      // cannot be, so it is the whole table.
      ...JPA_PALETTE_ACTIONS.map((a) => ({
        id: `jpa:${a.id}`,
        title: `JPA: ${a.title.toLowerCase()}…`,
        icon: 'wand',
        action: () => run(() => bennuUiStore.openJpaGenerate(a.id, projectStore.activeFilePath)),
        when: hasJpa,
      })),
    ];
    const idle = !!projectStore.project && !bennuRunStore.active;
    // A test run shares the backend's single-run lock with the build, so a test verb offered
    // while either is in flight would only be refused. Not gated on `javaTools` any more: a Cargo
    // project has a runner of its own, and gating on the ecosystem was what hid all of it.
    const testStore = activeTestStore();
    const testsIdle = idle && !testStore.running;
    const runItems = [
      { id: 'build', title: javaTools ? 'Build project' : 'Check project (cargo check)', icon: 'hammer', shortcut: 'Ctrl+F9',
        action: () => run(triggerBuild), when: idle },
      { id: 'validate', title: 'Validate project (no compile)', icon: 'list-checks',
        action: () => run(triggerValidate), when: idle && javaTools },
      { id: 'run', title: 'Run', icon: 'play', shortcut: 'Shift+F10',
        action: () => run(triggerRun), when: idle },
      // Debugging is JVM-only: `bennu_run` attaches JDWP to the child it spawned, and a cargo
      // command forks its own compiler and its own program.
      { id: 'debug', title: 'Debug', icon: 'bug', shortcut: 'Shift+F9',
        action: () => run(triggerDebug), when: idle && javaTools },
      // The three that only mean anything while the program is standing still. Offered from
      // the palette as well as the panel because the panel has to be open to press one, and
      // reaching a verb from wherever you are is what the palette is for.
      { id: 'dbgresume', title: 'Resume the program', icon: 'play', shortcut: 'F9',
        action: () => run(() => void bennuDebugStore.resume()), when: bennuDebugStore.paused },
      { id: 'dbgstepover', title: 'Step over', icon: 'play', shortcut: 'F8',
        action: () => run(() => void bennuDebugStore.step('over')), when: bennuDebugStore.paused },
      { id: 'dbgstepinto', title: 'Step into', icon: 'play', shortcut: 'F7',
        action: () => run(() => void bennuDebugStore.step('into')), when: bennuDebugStore.paused },
      { id: 'dbgstepout', title: 'Step out', icon: 'play', shortcut: 'Shift+F8',
        action: () => run(() => void bennuDebugStore.step('out')), when: bennuDebugStore.paused },
      { id: 'dbgbreak', title: 'Toggle breakpoint', icon: 'bug', shortcut: 'Ctrl+F8',
        action: () => run(toggleBreakpointAtCaret),
        when: javaTools && supportsCodeNav(projectStore.activeFilePath) },
      { id: 'dbglist', title: 'Breakpoints…', icon: 'bug', shortcut: 'Ctrl+Shift+F8',
        action: () => run(() => bennuUiStore.openBreakpoints()),
        when: javaTools && !!projectStore.project },
      { id: 'dbgmute', title: bennuDebugStore.muted ? 'Arm breakpoints' : 'Mute breakpoints',
        icon: 'bug',
        action: () => run(() => void bennuDebugStore.toggleMute()), when: bennuDebugStore.live },
      { id: 'dbgdetach', title: 'Detach the debugger', icon: 'bug',
        action: () => run(() => void bennuDebugStore.detachSession()), when: bennuDebugStore.live },
      { id: 'rerun', title: 'Rerun', icon: 'rerun',
        action: () => run(() => void bennuRunStore.rerunApp()), when: idle && bennuRunStore.canRerun },
      { id: 'stoprun', title: 'Stop the program', icon: 'hammer',
        action: () => run(() => void bennuRunStore.stop()), when: bennuRunStore.running },
      { id: 'runcfg', title: 'Edit run configuration…', icon: 'sliders',
        action: () => run(() => bennuUiStore.openRunConfig()), when: !!projectStore.project },
      // One entry per cargo command, so every one is reachable by name from the keyboard rather
      // than only by clicking a row in the panel — which has to be open to click. Aimed at the
      // whole workspace, since that is what "run cargo clippy" means with no crate in hand; a
      // single crate is the panel's row or a saved configuration.
      ...bennuCargoStore.commonCommands.map((c) => ({
        id: `cargo:${c.id}`,
        title: `Cargo: ${c.label} the workspace`,
        icon: 'cog',
        action: () =>
          run(() => {
            const root = projectStore.project?.root;
            if (!root) return;
            void bennuRunStore.runCargoCommand(
              root,
              { ...emptyCargoInvocation(c.id), workspace: true },
              `${c.id} workspace`,
            );
          }),
        when: idle && projectStore.isCargo,
      })),
      // Tests. Every one of these is also a button in the panel — but the panel has to be
      // open to press one, and the point of the palette is to reach a verb from wherever you
      // are. The caret verb is gated on the file actually declaring a test, so it is absent
      // rather than present-and-useless everywhere else.
      { id: 'test-all', title: 'Run all tests', icon: 'flask', shortcut: 'Ctrl+Shift+F5',
        action: () => run(triggerRunAllTests), when: testsIdle },
      { id: 'test-caret', title: 'Run test at caret', icon: 'play', shortcut: 'Ctrl+Shift+F10',
        action: () => run(() => void triggerRunTestAtCaret()), when: testsIdle && activeFileHasTests },
      { id: 'test-rerun', title: 'Rerun tests', icon: 'refresh-cw', shortcut: 'Ctrl+F5',
        action: () => run(() => void testStore.rerun()), when: testsIdle && testStore.hasResults },
      { id: 'test-rerun-failed', title: 'Rerun failed tests', icon: 'rerun',
        action: () => run(() => void testStore.rerunFailed()), when: testsIdle && testStore.hasFailures },
      { id: 'test-stop', title: 'Stop the test run', icon: 'hammer',
        action: () => run(() => void testStore.stop()), when: testStore.running },
      { id: 'hotswap-all', title: 'Deploy all JSPs to Tomcat', icon: 'server',
        action: () => run(() => void deployToTomcat(true)), when: !!projectStore.project && javaTools },
    ];
    // Switch project — one entry per other project in the ACTIVE workspace (keyboard-first).
    const projectSwitchItems = projectStore.hasWorkspace
      ? projectStore.workspaceProjects
          .filter((p) => p.root !== projectStore.project?.root)
          .map((p) => ({
            id: `psw:${p.root}`, title: `Switch to project ${p.name}`, icon: 'folder-tree',
            shortcut: undefined as string | undefined,
            action: () => run(() => void projectStore.switchProject(p.root)), when: true,
          }))
      : [];
    // Switch workspace — one entry per non-active workspace. Manage / New live in Application below.
    const workspaceItems = workspacesStore.hasMany
      ? workspacesStore.workspaces
          .filter((w) => w.id !== workspacesStore.activeId)
          .map((w) => ({
            id: `wss:${w.id}`, title: `Switch to workspace ${w.name || 'Workspace'}`, icon: 'folder-tree',
            shortcut: undefined as string | undefined,
            action: () => run(() => void workspacesStore.switchTo(w.id)), when: true,
          }))
      : [];
    const appItems = [
      { id: 'workspaces', title: 'Manage workspaces…', icon: 'folder-tree', action: () => run(() => bennuUiStore.openWorkspaceManager()), when: true },
      { id: 'newworkspace', title: 'New workspace…', icon: 'folder-tree',
        action: () => run(async () => { await workspacesStore.create('New workspace'); bennuUiStore.openWorkspaceManager(); }), when: true },
      { id: 'projectcfg', title: 'Project Configuration…', icon: 'sliders', action: () => run(() => bennuUiStore.openProjectConfig()), when: !!projectStore.project },
      { id: 'tomcatcfg', title: 'Tomcat hot-swap…', icon: 'server', action: () => run(() => bennuUiStore.openTomcatConfig()), when: !!projectStore.project && javaTools },
      { id: 'indexinspector', title: 'Index inspector…', icon: 'box', action: () => run(() => bennuUiStore.openIndexInspector()), when: !!projectStore.project && javaTools },
      { id: 'reindex', title: 'Rebuild index', icon: 'refresh-cw',
        action: () => run(() => { const r = projectStore.project?.root; if (r) void bennuIndexStore.rebuild(r); }),
        when: !!projectStore.project && javaTools && !bennuIndexStore.indexing },
      { id: 'docs', title: 'Documentation', icon: 'command', shortcut: 'F1', action: () => run(() => bennuUiStore.toggleDocs()), when: true },
      { id: 'settings', title: 'Settings', icon: 'command', shortcut: 'Ctrl+,', action: () => run(() => bennuUiStore.openSettings()), when: true },
      { id: 'about', title: 'About Bennu', icon: 'info', action: () => run(() => bennuUiStore.openAbout()), when: true },
    ];
    const pack = (items: typeof editorItems) =>
      items.filter((c) => c.when && (!q || c.title.toLowerCase().includes(q)))
        .map((c) => ({ id: c.id, title: c.title, icon: c.icon, shortcut: c.shortcut, action: c.action }));
    const out: PaletteSection[] = [];
    const ed = pack(editorItems); if (ed.length) out.push({ id: 'editor', label: 'Editor', items: ed });
    const rn = pack(runItems);    if (rn.length) out.push({ id: 'run', label: 'Run', items: rn });
    const vw = pack(viewItems);   if (vw.length) out.push({ id: 'view', label: 'View', items: vw });
    const ps = pack(projectSwitchItems); if (ps.length) out.push({ id: 'switch-project', label: 'Switch project', items: ps });
    const ws = pack(workspaceItems); if (ws.length) out.push({ id: 'switch-workspace', label: 'Switch workspace', items: ws });
    const ap = pack(appItems);    if (ap.length) out.push({ id: 'app', label: 'Application', items: ap });
    return out;
  });

  // ── Window-level keybindings ─────────────────────────────────────────────────
  function onKeyDown(e: KeyboardEvent) {
    // In the tabbed container this shell stays mounted (and subscribed) while
    // its tab is in the background — ignore keys unless we're the tab on
    // screen. No-op in a standalone Bennu window.
    if (!surfaceStore.hasFocus('bennu')) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && isKey(e, 'k')) { e.preventDefault(); bennuUiStore.togglePalette(); return; }
    if (bennuUiStore.paletteOpen) return; // the palette owns the keyboard while open

    // F1 toggles docs from anywhere; Docs/Settings/Find modals own Esc themselves.
    if (e.key === 'F1') { e.preventDefault(); bennuUiStore.toggleDocs(); return; }
    if (mod && e.key === ',') { e.preventDefault(); bennuUiStore.openSettings(); return; }

    // The Go-to navigator. One overlay over classes / files / symbols — the shortcut only
    // decides which tab it lands on, and Tab moves between them without reopening.
    // Ctrl+Shift+Y for symbols because IntelliJ's Ctrl+Alt+Shift+N is off-limits here:
    // Ctrl+Alt+<letter> is dropped by Chromium on IT/DE/FR/ES keyboards (AltGr).
    if (mod && e.shiftKey && !e.altKey && isKey(e, 'y')) {
      if (!projectStore.project) return;
      e.preventDefault();
      bennuUiStore.openNav('symbol', editor?.getSelectedText() ?? '');
      return;
    }
    // Expand / shrink the selection by one syntactic step, on the server's own idea of structure.
    // Alt+Shift+arrow matches VS Code; IntelliJ's Ctrl+W is not available here, because a WebView
    // may take it as "close the window" and losing the window is not a selection gesture.
    if (e.altKey && e.shiftKey && !mod && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
      if (!supportsCodeNav(projectStore.activeFilePath)) return;
      e.preventDefault();
      void (e.key === 'ArrowRight' ? editor?.expandSelection() : editor?.shrinkSelection());
      return;
    }
    if (mod && !e.altKey && isKey(e, 'n')) {
      if (!projectStore.project) return;
      e.preventDefault();
      // Seed the navigator from the editor selection (IntelliJ) — a highlighted word.
      bennuUiStore.openNav(e.shiftKey ? 'file' : 'class', editor?.getSelectedText() ?? '');
      return;
    }

    // Save the active file (Ctrl/Cmd+S).
    if (mod && !e.shiftKey && !e.altKey && isKey(e, 's')) {
      if (!projectStore.activeFilePath) return;
      e.preventDefault(); saveActive(); return;
    }
    // Rename (Shift+F6) — refactor the symbol under the caret with a preview.
    if (e.shiftKey && !mod && !e.altKey && e.key === 'F6') {
      if (!supportsCodeNav(projectStore.activeFilePath)) return;
      e.preventDefault(); editor?.openRename(); return;
    }

    // Build (Ctrl+F9) / Run (Shift+F10) — IntelliJ. Project-scoped; no-op while busy.
    if (mod && !e.shiftKey && !e.altKey && e.key === 'F9') {
      if (!projectStore.project || bennuRunStore.active) return;
      e.preventDefault(); triggerBuild(); return;
    }
    if (!mod && e.shiftKey && !e.altKey && e.key === 'F10') {
      // Run launches a run configuration — a Java main class. A Cargo project has none.
      if (!projectStore.project || !javaTools || bennuRunStore.active) return;
      e.preventDefault(); triggerRun(); return;
    }

    /*
     * The debugger, on IntelliJ's keys.
     *
     * F9 is two verbs one modifier apart, and deliberately so: Ctrl+F9 builds, plain F9
     * resumes. They can never be ambiguous because resume only exists while the program is
     * stopped, and the build guard above already took Ctrl+F9.
     */
    if (!mod && e.shiftKey && !e.altKey && e.key === 'F9') {
      if (!projectStore.project || !javaTools || bennuRunStore.active) return;
      e.preventDefault(); triggerDebug(); return;
    }
    if (!mod && !e.shiftKey && !e.altKey && e.key === 'F9' && bennuDebugStore.paused) {
      e.preventDefault(); void bennuDebugStore.resume(); return;
    }
    // Ctrl+Shift+F8 — the breakpoint list, IntelliJ's key. Before the step handlers, which
    // also claim F8 but never with Ctrl.
    if (mod && e.shiftKey && !e.altKey && e.key === 'F8') {
      if (!javaTools || !projectStore.project) return;
      e.preventDefault(); bennuUiStore.openBreakpoints(); return;
    }
    if (!mod && !e.altKey && e.key === 'F8' && bennuDebugStore.paused) {
      e.preventDefault(); void bennuDebugStore.step(e.shiftKey ? 'out' : 'over'); return;
    }
    if (!mod && !e.shiftKey && !e.altKey && e.key === 'F7' && bennuDebugStore.paused) {
      e.preventDefault(); void bennuDebugStore.step('into'); return;
    }
    // Ctrl+F8 works whether or not anything is running: a breakpoint is a property of the
    // project, and setting one before you launch is the ordinary case.
    if (mod && !e.shiftKey && !e.altKey && e.key === 'F8') {
      if (!javaTools || !supportsCodeNav(projectStore.activeFilePath)) return;
      e.preventDefault(); toggleBreakpointAtCaret(); return;
    }
    /*
     * Ctrl+Shift+F10 — IntelliJ's "run the thing in front of me". Which thing depends on
     * what is open, and the two readings are disjoint: on a JSP it deploys the page, on a
     * Java file that declares tests it runs the test at the caret. Anywhere else the key
     * stays silent rather than picking one of the two at random.
     */
    if (mod && e.shiftKey && !e.altKey && e.key === 'F10') {
      if (!projectStore.project) return;
      if (javaTools && isJspFile(projectStore.activeFilePath)) {
        e.preventDefault(); void deployToTomcat(false); return;
      }
      if (activeFileHasTests && !activeTestStore().running && !bennuRunStore.active) {
        e.preventDefault(); void triggerRunTestAtCaret(); return;
      }
      // A Cargo project has no deploy to fall back on: the key means the test at the caret, and
      // on a file with none it stays silent rather than doing something else.
      if (!javaTools) return;
      // A Java file with no tests: keep the historical behaviour (deploy) so the key still
      // does its old job everywhere it used to.
      e.preventDefault(); void deployToTomcat(false); return;
    }
    // Rerun the last test run (Ctrl+F5) / run them all (Ctrl+Shift+F5) — IntelliJ's Rerun.
    if (mod && !e.altKey && e.key === 'F5') {
      if (!projectStore.project || activeTestStore().running || bennuRunStore.active) return;
      e.preventDefault();
      if (e.shiftKey) triggerRunAllTests();
      else void activeTestStore().rerun();
      return;
    }

    /*
     * The hierarchies. IntelliJ's Ctrl+H for the type hierarchy, and Ctrl+Shift+H for the call
     * hierarchy rather than its Ctrl+Alt+H — Ctrl+Alt+<letter> is dropped by Chromium on IT/DE/FR/ES
     * layouts to preserve AltGr, so that binding would never fire here.
     *
     * Server-backed buffers only: Bennu's Java engine answers find-usages and has no hierarchy, so
     * on a `.java` the key would open a panel that could only say "nothing here".
     */
    if (mod && !e.altKey && isKey(e, 'h')) {
      if (!isLspFile(projectStore.activeFilePath)) return;
      e.preventDefault();
      if (e.shiftKey) editor?.showCallHierarchy();
      else editor?.showTypeHierarchy();
      return;
    }

    // Find in project (Ctrl+Shift+F) — a modal, replacing the old Search rail.
    if (mod && e.shiftKey && isKey(e, 'f')) { e.preventDefault(); bennuUiStore.openFind(editor?.getSelectedText() ?? ''); return; }

    // Structural search (Ctrl+Shift+M) — the shape-aware sibling of the one above. Java-only:
    // it needs a grammar, and on a Cargo project there is none to point it at.
    if (mod && e.shiftKey && !e.altKey && isKey(e, 'm') && javaTools) {
      e.preventDefault(); bennuUiStore.openSsr(); return;
    }

    // Workspace manager (Ctrl+Shift+W) — create / switch / manage named workspaces.
    if (mod && e.shiftKey && !e.altKey && isKey(e, 'w')) { e.preventDefault(); bennuUiStore.openWorkspaceManager(); return; }

    // Spring beans (Ctrl+Shift+B) — the framework catalog with a keyboard door, since it
    // is the one you reach for while reading code. Its siblings (Endpoints, Config) stay
    // palette-only. Silent on a project that declares none: no panel to open.
    if (mod && e.shiftKey && !e.altKey && isKey(e, 'b')) {
      if (!catalogIds.includes('beans')) return;
      e.preventDefault(); bennuUiStore.toggleBottom('beans'); return;
    }

    // File Structure popup (Ctrl+F12, IntelliJ) — a searchable quick-outline of the
    // active file (methods/fields for Java, element names for XML/JSP/HTML).
    if (mod && !e.shiftKey && !e.altKey && e.key === 'F12') {
      if (!supportsCodeNav(projectStore.activeFilePath)) return;
      e.preventDefault(); bennuUiStore.openFileStructure(); return;
    }

    // Terminal (Alt+F12, IntelliJ). Alt+digit tool toggles. Alt+Enter intentions,
    // Alt+Insert generate — both IntelliJ-consistent, editor-scoped (no-op with no
    // file open, guarded inside the editor's imperative methods).
    // Navigation history — IntelliJ's Ctrl+Alt+←/→ (back / forward through recent jumps).
    if (e.ctrlKey && e.altKey && !e.shiftKey) {
      if (e.key === 'ArrowLeft')  { e.preventDefault(); editor?.navBack(); return; }
      if (e.key === 'ArrowRight') { e.preventDefault(); editor?.navForward(); return; }
    }

    // Format with the language's own formatter (rustfmt for Rust). Alt+Shift+F rather than
    // IntelliJ's Ctrl+Alt+L: Chromium drops Ctrl+Alt+<letter> on IT/DE/FR/ES layouts to preserve
    // AltGr, so that binding would never fire on this machine.
    //
    // Every letter and digit below is matched with `isKey` rather than against `e.key`, and that is
    // load-bearing for exactly these Alt chords: macOS composes Option+<key> into another character
    // (Option+Shift+F is `Ï`, Option+Shift+M is `Â`, Option+1 is `¡`), so comparing the character
    // silently unbinds the whole Alt family on a Mac.
    if (e.altKey && e.shiftKey && !e.ctrlKey && !e.metaKey && isKey(e, 'f')) {
      if (!isLspFile(projectStore.activeFilePath)) return;
      e.preventDefault(); void editor?.formatDocument(); return;
    }

    // The module graph — who depends on whom inside the project. Alt+Shift+D ("dependency diagram")
    // rather than the obvious Ctrl+Shift+G: that one is `switch_window`, bound in EVERY window, and
    // taking it here would break escaping the window from inside Bennu. A dialog, so the chord only
    // has to open it — Esc closes it like every other one.
    if (e.altKey && e.shiftKey && !e.ctrlKey && !e.metaKey && isKey(e, 'd')) {
      if (!projectStore.project) return;
      e.preventDefault(); bennuUiStore.openModuleGraph(); return;
    }

    // The i18n panel — the translation under the caret, rendered. Alt+Shift+I, in the same safe
    // family as the two above, and gated on the file: on anything but a translation bundle the panel
    // has nothing to say, and opening an empty one is worse than the key doing nothing.
    if (e.altKey && e.shiftKey && !e.ctrlKey && !e.metaKey && isKey(e, 'i')) {
      if (!isI18nBundle(projectStore.activeFilePath)) return;
      e.preventDefault(); bennuUiStore.toggleRight('i18n'); return;
    }

    // Expand the macro at the caret. Alt+Shift+M, in the same safe family as the format binding
    // above and beside Ctrl+Shift+M (structural search) without colliding with it.
    if (e.altKey && e.shiftKey && !e.ctrlKey && !e.metaKey && isKey(e, 'm')) {
      if (!isLspFile(projectStore.activeFilePath)) return;
      e.preventDefault(); editor?.expandMacro(); return;
    }

    if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      if (e.key === 'F12') { e.preventDefault(); bennuUiStore.toggleBottom('terminal'); return; }
      if (e.key === 'F7') {
        if (!supportsCodeNav(projectStore.activeFilePath)) return;
        e.preventDefault(); void editor?.findUsages(); return;
      }
      if (isKey(e, '1')) { e.preventDefault(); bennuUiStore.toggleLeft('project'); return; }
      // Endpoints — the one framework catalog with a rail button, so it gets the rail's
      // digit like every other tool there, and only when the rail has it.
      if (isKey(e, '4') && catalogIds.includes('endpoints')) { e.preventDefault(); bennuUiStore.toggleBottom('endpoints'); return; }
      if (isKey(e, '6')) { e.preventDefault(); bennuUiStore.toggleBottom('problems'); return; }
      // Tests (Alt+5) — the CATALOGUE, on the right rail. A test RUN is a tab of the Run
      // console; this is where you go to start one. Java-only, like its rail icon.
      if (isKey(e, '5') && javaTools) { e.preventDefault(); bennuUiStore.toggleRight('tests'); return; }
      if (isKey(e, '7')) { e.preventDefault(); bennuUiStore.toggleBottom('todos'); return; }
      if (isKey(e, '0')) { e.preventDefault(); bennuUiStore.toggleBottom('build'); return; }
      // Both ecosystems, matching their rail icons: the Dependencies panel answers for a Cargo
      // workspace as well as a Maven reactor, and a cargo command streams into the same console.
      if (isKey(e, 'n')) { e.preventDefault(); bennuUiStore.toggleLeft('dependencies'); return; }
      // The Run console. A letter and not a digit because IntelliJ's Alt+4 is already Endpoints
      // here, and moving an existing tool's shortcut to make room would cost more than it buys.
      if (isKey(e, 'r')) { e.preventDefault(); bennuUiStore.toggleBottom('run'); return; }
      // The Java-only tools. Gated on `javaTools` for the same reason their rail icons and
      // palette entries are: on a Cargo project the shortcut would open a panel that can
      // only be empty, and whose toggle is nowhere on screen to close it again.
      if (javaTools) {
        if (isKey(e, '2')) { e.preventDefault(); bennuUiStore.toggleLeft('structure'); return; }
        // Forms needs pages, not just Java — same gate as its rail icon.
        if (isKey(e, '3') && jspTools) { e.preventDefault(); bennuUiStore.toggleBottom('forms'); return; }
        // Both views read Bennu's own engines, which have nothing to say about a Rust file —
        // see the rail item.
        if (isKey(e, '9')) { e.preventDefault(); bennuUiStore.toggleRight('ast'); return; }
      }
      // The build tool's window — Maven's goals or Cargo's crates, one slot. Outside the Java gate
      // because a Cargo project has one too.
      if (isKey(e, '8')) {
        e.preventDefault();
        bennuUiStore.toggleRight(projectStore.isCargo ? 'cargo' : 'maven');
        return;
      }
      /*
       * Intentions. Consumed **whatever the buffer is**, and that is the load-bearing part: with the
       * key left to fall through, the WebView's contenteditable takes Alt+Enter as a line break and
       * the gesture *edits the file* instead of doing nothing. A key that means "what can you do
       * here" must never insert anything.
       *
       * Both engines answer: Bennu's own intentions on a `.java`, a language server's code actions on
       * a file it owns — the same list, because the user's question is not "which engine".
       */
      if (e.key === 'Enter') {
        e.preventDefault();
        const path = projectStore.activeFilePath;
        if (isJavaFile(path) || isLspFile(path)) editor?.openIntentions();
        return;
      }
      if (e.key === 'Insert') {
        if (!isJavaFile(projectStore.activeFilePath)) return;
        e.preventDefault(); bennuUiStore.openGenerate(); return;
      }
    }

    if (mod && isKey(e, 'g')) { e.preventDefault(); editor?.openGoto(); return; }
    // Go to definition (Ctrl/Cmd+B, IntelliJ) — resolves the action reference under
    // the caret to its config/class/view. Editor-scoped; no-op with no file open.
    if (mod && !e.shiftKey && isKey(e, 'b')) {
      if (!supportsCodeNav(projectStore.activeFilePath)) return;
      e.preventDefault(); editor?.goToDefinition(); return;
    }
    if (mod && isKey(e, 'f')) { e.preventDefault(); editor?.openSearch(); return; }
    if (mod && isKey(e, 'o')) {
      e.preventDefault();
      window.dispatchEvent(new CustomEvent('bennu:open-project'));
      return;
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="shell">
  <BennuTitleBar />

  <div class="content-area">
    <WorkspaceShell>
      {#snippet leftRail()}
        <ActivityBar side="left" ariaLabel="Tool windows" topItems={leftTop} bottomItems={leftBottom} />
      {/snippet}
      {#snippet rightRail()}
        <ActivityBar side="right" ariaLabel="Inspection rail" topItems={rightTop} bottomItems={rightBottom} />
      {/snippet}

      {#snippet panels()}
        {#if showLeft}
          <PanelCard orientation="left" initialSize={260} minSize={180} maxSize={460}>
            {#if bennuUiStore.leftPanel === 'project'}<BennuSidebar />
            {:else if bennuUiStore.leftPanel === 'structure'}<BennuStructurePanel />
            {:else if bennuUiStore.leftPanel === 'dependencies'}<BennuDependenciesPanel />{/if}
          </PanelCard>
        {/if}

        <div class="main-col">
          <div class="card grow">
            <BennuEditor bind:this={editor} onGenerate={openGenerateFromIntention} />
          </div>
          {#if showJobOutput}
            <PanelCard orientation="bottom" initialSize={220} minSize={120} maxSize={560}>
              <JobOutputPanel />
            </PanelCard>
          {:else if showBottom}
            <PanelCard orientation="bottom" initialSize={220} minSize={120} maxSize={560}>
              <BennuBottomDock />
            </PanelCard>
          {/if}
        </div>

        {#if showRight}
          <!-- The i18n panel gets a wider default and a higher ceiling than the tool windows beside
               it. Those show lists of names; this one shows a SENTENCE, and at 280px a translation
               wraps into six lines — losing the one thing a preview is for. `{#key}` on the size so
               the card re-reads it when the active panel changes: `initialSize` is, by design, only
               read once. -->
          {#key wideRight}
          <PanelCard
            orientation="right"
            initialSize={wideRight ? 400 : 280}
            minSize={wideRight ? 280 : 200}
            maxSize={wideRight ? 760 : 520}
          >
            {#if bennuUiStore.rightPanel === 'maven'}<BennuMavenPanel />{/if}
            {#if bennuUiStore.rightPanel === 'cargo'}<BennuCargoPanel />{/if}
            <!-- The catalogue is per-ecosystem: a Rust test is identified by crate + target +
                 module + name, which is four levels the Java panel has no columns for. -->
            {#if bennuUiStore.rightPanel === 'tests'}
              {#if projectStore.isCargo}<BennuCargoTestsPanel />{:else}<BennuTestsCatalogPanel />{/if}
            {/if}
            {#if bennuUiStore.rightPanel === 'ast'}
              <SyntaxTreePanel
                title="Trees"
                source={bennuAstStore.source}
                tabs={bennuAstStore.views}
                activeTab={bennuAstStore.activeView}
                onTab={(id) => bennuAstStore.setActiveView(id)}
                emptyMessage="Open a file and its trees appear here."
              />
            {/if}
            <!-- Mounted only while shown, like Forms: it is scoped to the caret and re-read on every
                 move, so there is nothing to preserve — and while hidden it would keep asking the
                 backend about a panel nobody is looking at. -->
            {#if bennuUiStore.rightPanel === 'i18n'}<BennuI18nPanel />{/if}
          </PanelCard>
          {/key}
        {/if}
      {/snippet}
    </WorkspaceShell>
  </div>

  <BennuStatusBar>
    {#snippet footerExtra()}
      <!-- Keep the relevant status badges (jobs · notifications); the transfers
           (download/export) badge is dropped — Bennu never registers transfers,
           so without the `transfers` flag it stays hidden. -->
      <FeedbackStatusButtons />
    {/snippet}
  </BennuStatusBar>
</div>

{#if bennuUiStore.paletteOpen}
  <CommandPaletteShell
    onClose={() => bennuUiStore.closePalette()}
    {iconResolver}
    sections={paletteSections}
    bind:query={paletteQuery}
    placeholder="Type a command…"
  />
{/if}

{#if bennuUiStore.findOpen}
  <BennuFindInFilesModal onClose={() => bennuUiStore.closeFind()} />
{/if}

{#if bennuUiStore.settingsOpen}
  <BennuSettingsModal onClose={() => bennuUiStore.closeSettings()} />
{/if}

{#if bennuUiStore.projectConfigOpen}
  <BennuProjectConfigModal onClose={() => bennuUiStore.closeProjectConfig()} />
{/if}

{#if bennuUiStore.runConfigOpen}
  <BennuRunConfigModal onClose={() => bennuUiStore.closeRunConfig()} />
{/if}

{#if bennuUiStore.breakpointsOpen}
  <BennuBreakpointsModal onClose={() => bennuUiStore.closeBreakpoints()} />
{/if}

{#if bennuUiStore.ssrOpen}
  <BennuSsrModal onClose={() => bennuUiStore.closeSsr()} />
{/if}

{#if bennuUiStore.navOpen}
  <BennuGotoModal onClose={() => bennuUiStore.closeNav()} />
{/if}

{#if bennuUiStore.fileStructureOpen}
  <BennuFileStructureModal onClose={() => bennuUiStore.closeFileStructure()} />
{/if}

{#if bennuUiStore.indexInspectorOpen}
  <BennuIndexInspectorModal onClose={() => bennuUiStore.closeIndexInspector()} />
{/if}

{#if bennuUiStore.moduleGraphOpen}
  <BennuModuleGraphModal onClose={() => bennuUiStore.closeModuleGraph()} />
{/if}

{#if bennuUiStore.mojibakeScanOpen}
  <BennuMojibakeScanModal onClose={() => bennuUiStore.closeMojibakeScan()} />
{/if}

{#if bennuUiStore.tomcatConfigOpen}
  <BennuTomcatConfigModal onClose={() => bennuUiStore.closeTomcatConfig()} />
{/if}

{#if bennuUiStore.aboutOpen}
  <BennuAboutModal onClose={() => bennuUiStore.closeAbout()} />
{/if}

{#if bennuUiStore.generateOpen}
  <BennuGenerateModal
    mode={bennuUiStore.generateMode}
    onClose={() => bennuUiStore.closeGenerate()}
    onInsert={(text) => { editor?.insertAtCursor(text); editor?.focusEditor(); }}
  />
{/if}

{#if bennuUiStore.jpaGenerateOpen}
  <BennuJpaGenerateModal
    action={bennuUiStore.jpaGenerateAction}
    onClose={() => { bennuUiStore.closeJpaGenerate(); editor?.focusEditor(); }}
  />
{/if}

{#if bennuUiStore.validationCreatorOpen}
  <BennuValidationModal onClose={() => bennuUiStore.closeValidationCreator()} />
{/if}

{#if bennuUiStore.workspaceManagerOpen}
  <BennuWorkspaceManagerModal onClose={() => bennuUiStore.closeWorkspaceManager()} />
{/if}

<!-- Mounted by the window rather than by the Cargo panel: it is reachable from the palette, and the
     panel it is launched from need not be open for that. -->
{#if bennuUiStore.cargoAddOpen && projectStore.project && projectStore.isCargo}
  <BennuCargoAddModal
    root={projectStore.project.root}
    onClose={() => bennuUiStore.closeCargoAdd()}
  />
{/if}

<!-- Alt+Enter intentions popup. Owns its own visibility via bennuIntentionsStore;
     mounted unconditionally. On close it returns focus to the editor. -->
<BennuIntentionsOverlay onClose={() => editor?.focusEditor()} />

<!-- "This file changed on disk while you had unsaved edits" — the one interruption Bennu
     owes the user, since neither version can be discarded silently. Owns its visibility from
     the project store's conflict set; mounted unconditionally. -->
<BennuExternalChangeModal />

<!-- Alt+F7 find-usages popover — owns its visibility via bennuRefactorStore. -->
<BennuUsagesPopover />

{#if bennuRefactorStore.renameOpen}
  <BennuRenameModal onClose={() => { bennuRefactorStore.closeRename(); editor?.focusEditor(); }} />
{/if}

{#if bennuUiStore.docsOpen}
  <BennuDocsPanel onClose={() => bennuUiStore.closeDocs()} />
{/if}

<Tooltip />

{#if bennuContextMenuStore.open}
  <ContextMenu
    items={bennuContextMenuStore.items}
    x={bennuContextMenuStore.x}
    y={bennuContextMenuStore.y}
    onSelect={(id) => bennuContextMenuStore.select(id)}
    onClose={() => bennuContextMenuStore.close()}
  />
{/if}

<!-- Feedback surface for the Bennu window: toasts + notifications + progress
     addressed to this window via target="bennu". -->
<FeedbackHost id="bennu" />

<style>
  .shell {
    position: fixed; inset: 0;
    display: flex; flex-direction: column;
    background: var(--bg-base);
    overflow: hidden;
  }
  /* A few px of bg-elevated between the titlebar and the tabbed editor pane, so
     the floating panel cards read as detached from the chrome (IntelliJ New UI
     floating look). WorkspaceShell intentionally has no top padding; we add it
     here at the window level, tinted like the rail/titlebar so it's the "gap". */
  .content-area {
    flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden;
    padding-top: 5px;
    background: var(--bg-elevated);
  }

  /* The bg-elevated .workspace + inset .panels live in the shared WorkspaceShell;
     this owns only Bennu's arrangement inside the panels snippet. */
  .main-col { display: flex; flex-direction: column; flex: 1; min-width: 0; overflow: hidden; gap: 4px; }
  .card {
    display: flex; flex-shrink: 0;
    min-width: 0; min-height: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .card.grow { flex: 1; }
  .card.grow > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
