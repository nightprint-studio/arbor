<script lang="ts">
  /**
   * BennuWindow — the standalone Java-editor window shell.
   *
   * Boots the theme/appearance/animation config locally (each window is its own JS
   * context, so AppShell's onMount never runs here — mirrors MerulaWindow), then
   * composes Arbor's standard IntelliJ-New-UI frame like Corvus/Merula:
   *   TitleBar (project · … · run/debug · palette · docs · settings) + a bg-elevated
   *   WorkspaceShell with left/right activity rails + floating bg-base panel cards +
   *   a BOTTOM dock (Problems · Terminal) + the footer status bar.
   *
   * Tool windows (IntelliJ New UI):
   *   • LEFT rail top     — Project (tree), Structure (symbols), Dependencies — left side panels.
   *   • LEFT rail bottom  — the bottom-dock toggles (Build, Problems, TODO, Terminal). Docs &
   *                         Settings live in the titlebar's right cluster.
   *   • RIGHT rail        — Maven (top); Services + the Forms toggle (bottom).
   *   • BOTTOM dock       — Build · Problems · TODO · Forms · Terminal, tabbed.
   * Find-in-project is a modal (Ctrl+Shift+F / palette), not a rail tool.
   */
  import { onMount } from 'svelte';
  import {
    Command, FolderTree, ListTree, Search, Hash, FileCode2, AlertTriangle,
    TerminalSquare, Hammer, Server, Wand2, Lightbulb, SlidersHorizontal, Info,
    Library, Target, Play, ListTodo, Box, RotateCw, IndentIncrease, ShieldCheck,
    TextCursorInput,
  } from 'lucide-svelte';

  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { signalWindowReady } from '$lib/ipc/window';

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
  import BennuServicesPanel from './BennuServicesPanel.svelte';
  import BennuBottomDock from './BennuBottomDock.svelte';
  import BennuEditor from './BennuEditor.svelte';
  import BennuDocsPanel from './BennuDocsPanel.svelte';
  import BennuSettingsModal from './BennuSettingsModal.svelte';
  import BennuFindInFilesModal from './BennuFindInFilesModal.svelte';
  import BennuProjectConfigModal from './BennuProjectConfigModal.svelte';
  import BennuAboutModal from './BennuAboutModal.svelte';
  import BennuGenerateModal from './BennuGenerateModal.svelte';
  import BennuValidationModal from './BennuValidationModal.svelte';
  import BennuIntentionsOverlay from './BennuIntentionsOverlay.svelte';
  import BennuRunConfigModal from './BennuRunConfigModal.svelte';
  import BennuRenameModal from './BennuRenameModal.svelte';
  import BennuUsagesPopover from './BennuUsagesPopover.svelte';
  import BennuGotoModal from './BennuGotoModal.svelte';
  import BennuIndexInspectorModal from './BennuIndexInspectorModal.svelte';
  import BennuFileStructureModal from './BennuFileStructureModal.svelte';
  import type { GenerateMode } from './bennu-intentions';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    // Subscribe to the build/run + index-progress event streams for this window;
    // detach on unmount.
    let detachRun: (() => void) | undefined;
    let detachIndex: (() => void) | undefined;
    let detachSpell: (() => void) | undefined;
    void bennuRunStore.attach().then((d) => { detachRun = d; });
    void bennuIndexStore.attach().then((d) => { detachIndex = d; });
    void bennuSpellStore.attach().then((d) => { detachSpell = d; });
    // Anti-white-flash: reveal this window once the first real frame is painted.
    requestAnimationFrame(() => requestAnimationFrame(() => void signalWindowReady().catch(() => {})));
    return () => { detachRun?.(); detachIndex?.(); detachSpell?.(); bennuIndexStore.reset(); };
  });

  // When a real (non-demo) project opens, kick off the indexing status + job. The BE
  // rebuilds the index on every open, so this fires each time the root changes.
  let lastIndexedRoot: string | null = null;
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    const demo = projectStore.isDemo;
    if (root && !demo && root !== lastIndexedRoot) {
      lastIndexedRoot = root;
      bennuIndexStore.onProjectOpen(root);
    }
  });

  // Project-level diagnostics (JDK status + wrong-encoding files) for the titlebar badge +
  // the Problems panel. Re-fetch when the project changes or the index (re)builds — the
  // encoding report lands after the project phase, `buildRevision` catches each phase.
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    void bennuIndexStore.buildRevision; // re-run as the (re)build progresses
    if (root && !projectStore.isDemo) void bennuDiagnosticsStore.refresh(root);
    else bennuDiagnosticsStore.reset();
  });

  // ── Build / Run triggers (mirror the titlebar; shared by keybindings + palette) ─
  function triggerBuild() {
    const root = projectStore.project?.root;
    if (root) void bennuRunStore.build(root);
  }
  function triggerRun() {
    const root = projectStore.project?.root;
    if (!root) return;
    // Honor the ACTIVE run configuration (main class + program args); open the editor
    // when nothing is configured yet.
    void bennuRunStore.runActive(root).then((ran) => { if (!ran) bennuUiStore.openRunConfig(); });
  }

  let editor = $state<{
    openGoto: () => void;
    openSearch: () => void;
    focusEditor: () => void;
    openIntentions: () => void;
    goToDefinition: () => void;
    openRename: () => void;
    findUsages: () => void;
    insertAtCursor: (text: string) => void;
  } | null>(null);

  /** Ctrl+S — save the active file to disk. */
  function saveActive() {
    void projectStore.saveActive().then((ok) => { if (ok) toastStore.show('Saved', 'success'); });
  }

  // Alt+Enter "Generate…" intention → open the Generate modal in that mode.
  function openGenerateFromIntention(mode: GenerateMode) {
    bennuUiStore.openGenerate(mode);
  }

  // ── Left/right rail items ────────────────────────────────────────────────────
  const leftTop = $derived<ActivityRailItem[]>([
    { id: 'project',   tooltip: 'Project',   shortcut: 'Alt+1', icon: FolderTree, active: bennuUiStore.leftPanel === 'project',   onclick: () => bennuUiStore.toggleLeft('project') },
    { id: 'structure', tooltip: 'Structure', shortcut: 'Alt+2', icon: ListTree,   active: bennuUiStore.leftPanel === 'structure', onclick: () => bennuUiStore.toggleLeft('structure') },
    { id: 'dependencies', tooltip: 'Dependencies', shortcut: 'Alt+N', icon: Library, active: bennuUiStore.leftPanel === 'dependencies', onclick: () => bennuUiStore.toggleLeft('dependencies') },
  ]);
  // Left rail bottom cluster: only the bottom-dock toggles (Terminal, Problems).
  // Docs/Settings moved to the titlebar's right cluster (IntelliJ/Corvus layout).
  // These drive the BOTTOM dock (BennuBottomDock), not a side panel — the active
  // state mirrors the dock's open tab.
  const leftBottom = $derived<ActivityRailItem[]>([
    { id: 'build',    tooltip: 'Build', shortcut: 'Alt+0',      icon: Hammer,         active: bennuUiStore.bottomPanel === 'build',    onclick: () => bennuUiStore.toggleBottom('build') },
    { id: 'problems', tooltip: 'Problems', shortcut: 'Alt+6',   icon: AlertTriangle,  active: bennuUiStore.bottomPanel === 'problems', onclick: () => bennuUiStore.toggleBottom('problems') },
    { id: 'todos',    tooltip: 'TODO', shortcut: 'Alt+7',       icon: ListTodo,       active: bennuUiStore.bottomPanel === 'todos',    onclick: () => bennuUiStore.toggleBottom('todos') },
    { id: 'terminal', tooltip: 'Terminal', shortcut: 'Alt+F12', icon: TerminalSquare, active: bennuUiStore.bottomPanel === 'terminal', onclick: () => bennuUiStore.toggleBottom('terminal') },
  ]);
  const rightTop = $derived<ActivityRailItem[]>([
    { id: 'maven', tooltip: 'Maven', shortcut: 'Alt+8', icon: Hammer, active: bennuUiStore.rightPanel === 'maven', onclick: () => bennuUiStore.toggleRight('maven') },
  ]);
  // Forms drives the BOTTOM dock (wide, horizontal data), not a side panel — its toggle sits
  // in the right rail's bottom cluster; the active state mirrors the dock's open tab.
  const rightBottom = $derived<ActivityRailItem[]>([
    { id: 'forms', tooltip: 'Forms', shortcut: 'Alt+3', icon: TextCursorInput, active: bennuUiStore.bottomPanel === 'forms', onclick: () => bennuUiStore.toggleBottom('forms') },
    { id: 'services', tooltip: 'Services', shortcut: 'Alt+9', icon: Server, active: bennuUiStore.rightPanel === 'services', onclick: () => bennuUiStore.toggleRight('services') },
  ]);

  const showLeft   = $derived(bennuUiStore.leftPanel !== null);
  const showRight  = $derived(bennuUiStore.rightPanel !== null);
  const showBottom = $derived(bennuUiStore.bottomPanel !== null);

  // ── Command palette ────────────────────────────────────────────────────────
  let paletteQuery = $state('');

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
    'play': Play as unknown as IconComponent,
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
  };
  function iconResolver(name: string): IconComponent { return ICONS[name] ?? ICONS.command; }

  function run(fn: () => void) { bennuUiStore.closePalette(); queueMicrotask(fn); }

  const paletteSections = $derived.by<PaletteSection[]>(() => {
    const q = paletteQuery.trim().toLowerCase();
    const editorItems = [
      { id: 'goto', title: 'Go to line', icon: 'hash', shortcut: 'Ctrl+G',
        action: () => run(() => editor?.openGoto()), when: !!projectStore.activeFilePath },
      { id: 'gotodef', title: 'Go to declaration', icon: 'target', shortcut: 'Ctrl+B',
        action: () => run(() => editor?.goToDefinition()), when: !!projectStore.activeFilePath },
      { id: 'gotoclass', title: 'Go to class…', icon: 'box', shortcut: 'Ctrl+N',
        action: () => run(() => bennuUiStore.openNav('class')), when: !!projectStore.project },
      { id: 'gotofile', title: 'Go to file…', icon: 'file', shortcut: 'Ctrl+Shift+N',
        action: () => run(() => bennuUiStore.openNav('file')), when: !!projectStore.project },
      { id: 'filestructure', title: 'File structure…', icon: 'list-tree', shortcut: 'Ctrl+F12',
        action: () => run(() => bennuUiStore.openFileStructure()), when: !!projectStore.activeFilePath },
      { id: 'usages', title: 'Find usages', icon: 'search', shortcut: 'Alt+F7',
        action: () => run(() => void editor?.findUsages()), when: !!projectStore.activeFilePath },
      { id: 'rename', title: 'Rename…', icon: 'target', shortcut: 'Shift+F6',
        action: () => run(() => editor?.openRename()), when: !!projectStore.activeFilePath },
      { id: 'save', title: 'Save file', icon: 'file', shortcut: 'Ctrl+S',
        action: () => run(saveActive), when: !!projectStore.activeFilePath },
      { id: 'find', title: 'Find in file', icon: 'search', shortcut: 'Ctrl+F',
        action: () => run(() => editor?.openSearch()), when: !!projectStore.activeFilePath },
      { id: 'findproj', title: 'Find in project', icon: 'search', shortcut: 'Ctrl+Shift+F',
        action: () => run(() => bennuUiStore.openFind()), when: true },
      { id: 'reveal', title: 'Select opened file in tree', icon: 'folder-tree',
        action: () => run(() => bennuUiStore.revealActiveInTree()), when: !!projectStore.activeFilePath },
      { id: 'generate', title: 'Generate…', icon: 'wand', shortcut: 'Alt+Insert',
        action: () => run(() => bennuUiStore.openGenerate()), when: !!projectStore.activeFilePath },
      { id: 'intentions', title: 'Show intentions', icon: 'bulb', shortcut: 'Alt+Enter',
        action: () => run(() => editor?.openIntentions()), when: !!projectStore.activeFilePath },
      { id: 'newvalidator', title: 'New Struts validator…', icon: 'shield',
        action: () => run(() => bennuUiStore.openValidationCreator()),
        when: projectStore.activeFilePath?.toLowerCase().endsWith('-validation.xml') ?? false },
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
      { id: 'structure', title: 'Toggle Structure', icon: 'list-tree',   shortcut: 'Alt+2', action: () => run(() => bennuUiStore.toggleLeft('structure')), when: true },
      { id: 'forms',     title: 'Toggle Forms',     icon: 'list',        shortcut: 'Alt+3', action: () => run(() => bennuUiStore.toggleBottom('forms')), when: true },
      { id: 'dependencies', title: 'Dependencies',  icon: 'library',     shortcut: 'Alt+N', action: () => run(() => bennuUiStore.toggleLeft('dependencies')), when: true },
      { id: 'problems',  title: 'Toggle Problems',  icon: 'alert',       shortcut: 'Alt+6', action: () => run(() => bennuUiStore.toggleBottom('problems')), when: true },
      { id: 'todos',     title: 'Toggle TODO',      icon: 'todo',        shortcut: 'Alt+7', action: () => run(() => bennuUiStore.toggleBottom('todos')), when: true },
      { id: 'terminal',  title: 'Toggle Terminal',  icon: 'terminal',    shortcut: 'Alt+F12', action: () => run(() => bennuUiStore.toggleBottom('terminal')), when: true },
      { id: 'maven',     title: 'Toggle Maven',     icon: 'hammer',      shortcut: 'Alt+8', action: () => run(() => bennuUiStore.toggleRight('maven')), when: true },
      { id: 'services',  title: 'Toggle Services',  icon: 'server',      shortcut: 'Alt+9', action: () => run(() => bennuUiStore.toggleRight('services')), when: true },
    ];
    const runItems = [
      { id: 'build', title: 'Build project', icon: 'hammer', shortcut: 'Ctrl+F9',
        action: () => run(triggerBuild), when: !!projectStore.project && !bennuRunStore.active },
      { id: 'run', title: 'Run', icon: 'play', shortcut: 'Shift+F10',
        action: () => run(triggerRun), when: !!projectStore.project && !bennuRunStore.active },
      { id: 'stoprun', title: 'Stop', icon: 'hammer',
        action: () => run(() => void bennuRunStore.stop()), when: bennuRunStore.running },
      { id: 'runcfg', title: 'Edit run configuration…', icon: 'sliders',
        action: () => run(() => bennuUiStore.openRunConfig()), when: !!projectStore.project },
    ];
    const appItems = [
      { id: 'projectcfg', title: 'Project Configuration…', icon: 'sliders', action: () => run(() => bennuUiStore.openProjectConfig()), when: !!projectStore.project },
      { id: 'indexinspector', title: 'Index inspector…', icon: 'box', action: () => run(() => bennuUiStore.openIndexInspector()), when: !!projectStore.project },
      { id: 'reindex', title: 'Rebuild index', icon: 'refresh-cw',
        action: () => run(() => { const r = projectStore.project?.root; if (r) void bennuIndexStore.rebuild(r); }),
        when: !!projectStore.project && !bennuIndexStore.indexing },
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
    const ap = pack(appItems);    if (ap.length) out.push({ id: 'app', label: 'Application', items: ap });
    return out;
  });

  // ── Window-level keybindings ─────────────────────────────────────────────────
  function onKeyDown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); bennuUiStore.togglePalette(); return; }
    if (bennuUiStore.paletteOpen) return; // the palette owns the keyboard while open

    // F1 toggles docs from anywhere; Docs/Settings/Find modals own Esc themselves.
    if (e.key === 'F1') { e.preventDefault(); bennuUiStore.toggleDocs(); return; }
    if (mod && e.key === ',') { e.preventDefault(); bennuUiStore.openSettings(); return; }

    // Go to Class (Ctrl+N) / Go to File (Ctrl+Shift+N) — the quick-open navigator.
    if (mod && !e.altKey && e.key.toLowerCase() === 'n') {
      if (!projectStore.project) return;
      e.preventDefault();
      bennuUiStore.openNav(e.shiftKey ? 'file' : 'class');
      return;
    }

    // Save the active file (Ctrl/Cmd+S).
    if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 's') {
      if (!projectStore.activeFilePath) return;
      e.preventDefault(); saveActive(); return;
    }
    // Rename (Shift+F6) — refactor the symbol under the caret with a preview.
    if (e.shiftKey && !mod && !e.altKey && e.key === 'F6') {
      if (!projectStore.activeFilePath) return;
      e.preventDefault(); editor?.openRename(); return;
    }

    // Build (Ctrl+F9) / Run (Shift+F10) — IntelliJ. Project-scoped; no-op while busy.
    if (mod && !e.shiftKey && !e.altKey && e.key === 'F9') {
      if (!projectStore.project || bennuRunStore.active) return;
      e.preventDefault(); triggerBuild(); return;
    }
    if (!mod && e.shiftKey && !e.altKey && e.key === 'F10') {
      if (!projectStore.project || bennuRunStore.active) return;
      e.preventDefault(); triggerRun(); return;
    }

    // Find in project (Ctrl+Shift+F) — a modal, replacing the old Search rail.
    if (mod && e.shiftKey && e.key.toLowerCase() === 'f') { e.preventDefault(); bennuUiStore.openFind(); return; }

    // File Structure popup (Ctrl+F12, IntelliJ) — a searchable quick-outline of the
    // active file (methods/fields for Java, element names for XML/JSP/HTML).
    if (mod && !e.shiftKey && !e.altKey && e.key === 'F12') {
      if (!projectStore.activeFilePath) return;
      e.preventDefault(); bennuUiStore.openFileStructure(); return;
    }

    // Terminal (Alt+F12, IntelliJ). Alt+digit tool toggles. Alt+Enter intentions,
    // Alt+Insert generate — both IntelliJ-consistent, editor-scoped (no-op with no
    // file open, guarded inside the editor's imperative methods).
    if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      if (e.key === 'F12') { e.preventDefault(); bennuUiStore.toggleBottom('terminal'); return; }
      if (e.key === 'F7') {
        if (!projectStore.activeFilePath) return;
        e.preventDefault(); void editor?.findUsages(); return;
      }
      if (e.key === '1') { e.preventDefault(); bennuUiStore.toggleLeft('project'); return; }
      if (e.key === '2') { e.preventDefault(); bennuUiStore.toggleLeft('structure'); return; }
      if (e.key === '3') { e.preventDefault(); bennuUiStore.toggleBottom('forms'); return; }
      if (e.key.toLowerCase() === 'n') { e.preventDefault(); bennuUiStore.toggleLeft('dependencies'); return; }
      if (e.key === '6') { e.preventDefault(); bennuUiStore.toggleBottom('problems'); return; }
      if (e.key === '7') { e.preventDefault(); bennuUiStore.toggleBottom('todos'); return; }
      if (e.key === '0') { e.preventDefault(); bennuUiStore.toggleBottom('build'); return; }
      if (e.key === '8') { e.preventDefault(); bennuUiStore.toggleRight('maven'); return; }
      if (e.key === '9') { e.preventDefault(); bennuUiStore.toggleRight('services'); return; }
      if (e.key === 'Enter') {
        if (!projectStore.activeFilePath) return;
        e.preventDefault(); editor?.openIntentions(); return;
      }
      if (e.key === 'Insert') {
        if (!projectStore.activeFilePath) return;
        e.preventDefault(); bennuUiStore.openGenerate(); return;
      }
    }

    if (mod && e.key.toLowerCase() === 'g') { e.preventDefault(); editor?.openGoto(); return; }
    // Go to definition (Ctrl/Cmd+B, IntelliJ) — resolves the action reference under
    // the caret to its config/class/view. Editor-scoped; no-op with no file open.
    if (mod && !e.shiftKey && e.key.toLowerCase() === 'b') {
      if (!projectStore.activeFilePath) return;
      e.preventDefault(); editor?.goToDefinition(); return;
    }
    if (mod && e.key.toLowerCase() === 'f') { e.preventDefault(); editor?.openSearch(); return; }
    if (mod && e.key.toLowerCase() === 'o') {
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
          {#if showBottom}
            <PanelCard orientation="bottom" initialSize={220} minSize={120} maxSize={560}>
              <BennuBottomDock />
            </PanelCard>
          {/if}
        </div>

        {#if showRight}
          <PanelCard orientation="right" initialSize={280} minSize={200} maxSize={520}>
            {#if bennuUiStore.rightPanel === 'maven'}<BennuMavenPanel />
            {:else if bennuUiStore.rightPanel === 'services'}<BennuServicesPanel />{/if}
          </PanelCard>
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

{#if bennuUiStore.navOpen}
  <BennuGotoModal onClose={() => bennuUiStore.closeNav()} />
{/if}

{#if bennuUiStore.fileStructureOpen}
  <BennuFileStructureModal onClose={() => bennuUiStore.closeFileStructure()} />
{/if}

{#if bennuUiStore.indexInspectorOpen}
  <BennuIndexInspectorModal onClose={() => bennuUiStore.closeIndexInspector()} />
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

{#if bennuUiStore.validationCreatorOpen}
  <BennuValidationModal
    onClose={() => bennuUiStore.closeValidationCreator()}
    onInsert={(text) => { editor?.insertAtCursor(text); editor?.focusEditor(); }}
  />
{/if}

<!-- Alt+Enter intentions popup. Owns its own visibility via bennuIntentionsStore;
     mounted unconditionally. On close it returns focus to the editor. -->
<BennuIntentionsOverlay onClose={() => editor?.focusEditor()} />

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
