<script lang="ts">
  /**
   * NemusShell — the standalone music live-coding DAW shell (Step 0, mocked).
   * Mirrors Arbor's AppShell layout language: a bg-elevated workspace with
   * floating bg-base cards inset by 4px gaps (IntelliJ feel), the icon rails
   * flush to the edges, the SplitView (read-only arrangement ↔ tab editor) over
   * the bottom panel, and the footer. Honors zen + collapse toggles.
   *
   * Reuses Arbor's shell pieces (ActivityBar, ResizablePanel, WindowControls,
   * tooltips) for consistency + zero duplication; the nemus domain UI (panels,
   * arrangement, editor) lives under components/nemus/.
   */
  import {
    Files, ListTree, Music4, Terminal, AlertTriangle,
    SlidersHorizontal, Crosshair, BookOpen, Boxes, Piano,
  } from 'lucide-svelte';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import ResizablePanel from '$lib/components/layout/ResizablePanel.svelte';
  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';

  import NemusTitleBar from './shell/NemusTitleBar.svelte';
  import NemusFooter from './shell/NemusFooter.svelte';

  import FilesPanel from './panels/FilesPanel.svelte';
  import OutlinePanel from './panels/OutlinePanel.svelte';
  import SoundBankPanel from './panels/SoundBankPanel.svelte';
  import ConsolePanel from './panels/ConsolePanel.svelte';
  import ProblemsPanel from './panels/ProblemsPanel.svelte';
  import JobsPanel from './panels/JobsPanel.svelte';
  import MixerPanel from './panels/MixerPanel.svelte';
  import InspectorPanel from './panels/InspectorPanel.svelte';
  import DocsPanel from './panels/DocsPanel.svelte';

  import ArrangementView from './viz/ArrangementView.svelte';
  import TabbedEditor from './editor/TabbedEditor.svelte';
  import UsagesPopover from './editor/UsagesPopover.svelte';
  import StructurePopover from './editor/StructurePopover.svelte';

  import { onMount, onDestroy, type Snippet } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { nemusStore } from './nemus-store.svelte';
  import { nemusEngine } from './stores/engine.svelte';
  import { configStore } from './stores/config.svelte';
  import { packsStore } from './stores/packs.svelte';
  import { modelsStore } from './stores/models.svelte';
  import { workspaceStore } from './stores/workspace.svelte';
  import { projectStore } from './stores/project.svelte';
  import { fileWatchStore } from './stores/file-watch.svelte';
  import { onFsChanged } from '$lib/ipc/fs';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { projectActions } from './stores/project-actions.svelte';
  import { importActions } from './stores/import-actions.svelte';
  import { mixerStore } from './stores/mixer.svelte';
  import { referenceStore } from './stores/reference.svelte';
  import { soundsStore } from './stores/sounds.svelte';
  import { arrangementStore } from './viz/arrangement.svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import NemusProjectActions from './shell/NemusProjectActions.svelte';
  import NemusImportActions from './shell/NemusImportActions.svelte';
  import NemusSettingsModal from './shell/NemusSettingsModal.svelte';
  import NemusShortcutsModal from './shell/NemusShortcutsModal.svelte';
  import NemusCommandPalette from './shell/NemusCommandPalette.svelte';
  import InstrumentPreviewPanel from './preview/InstrumentPreviewPanel.svelte';
  import { NEMUS_BINDINGS, matchesNemus } from './nemus-keybindings';

  // Arbor-specific feedback badges (jobs · notifications) injected by the bridge
  // (NemusWindow) and rendered in the footer's right cluster — keeps NemusShell
  // and NemusFooter free of Arbor store imports (extractability).
  let { footerExtra }: { footerExtra?: Snippet } = $props();

  let unEngine: UnlistenFn | null = null;
  let unPacks:  UnlistenFn | null = null;
  let unModels: UnlistenFn | null = null;
  let unFsWatch: UnlistenFn | null = null;

  onMount(async () => {
    // Live engine + sample-pack + transcription-model streams (each nemus window
    // owns its listeners).
    unEngine = await nemusEngine.subscribe();
    unPacks  = await packsStore.subscribe();
    unModels = await modelsStore.subscribe();
    // External-change detection for the open .nemus file (IDE-style reload prompt).
    unFsWatch = await onFsChanged(() => void fileWatchStore.onChanged());
    void configStore.loadConfig();
    void packsStore.refresh();
    void modelsStore.refresh();
    // The DSL reference catalogue (autocomplete + hover + Docs panel). Static —
    // loaded once; failure leaves the editor working, just without language hints.
    void referenceStore.load();
    // The resolvable instrument registry powers `inst("…")` autocomplete — load
    // it up front so completions work without opening the Sound bank panel.
    void soundsStore.refresh();
    // Restore the persisted layout, then best-effort reopen the last project.
    await workspaceStore.load();
    nemusStore.applyLayout(workspaceStore.layout);
    if (workspaceStore.lastProject) {
      projectStore.open(workspaceStore.lastProject).catch(() => {});
    }
  });

  onDestroy(() => {
    unEngine?.();
    unPacks?.();
    unModels?.();
    unFsWatch?.();
    fileWatchStore.stop();
  });

  // Watch the directory of the active file for external edits (re-armed on tab
  // switch / cross-file open).
  $effect(() => {
    void projectStore.activeFilePath;
    void fileWatchStore.watchActive();
  });

  // Mirror layout changes to the persisted window state (debounced in the
  // store). Read the snapshot inside the effect so it tracks the panel state.
  $effect(() => {
    const snap = nemusStore.layoutSnapshot();
    workspaceStore.persistLayout(snap);
  });

  // Bridge the shared Jobs overlay's "View output" button into nemus. That button
  // (in the shared JobsOverlay, mounted here via the footer badge) targets the
  // main-app bottom-panel system (`uiStore.activeBottomSection`), which the nemus
  // window doesn't use — so it appeared to do nothing. Watch that one-shot signal
  // and open nemus's own Jobs panel instead; the overlay already set the active
  // job on the shared `jobsStore`, so the panel drills straight into its output.
  $effect(() => {
    if (uiStore.activeBottomSection === 'jobs') {
      uiStore.setActiveBottomSection(null); // consume the signal (nemus ignores it otherwise)
      nemusStore.showBottom('jobs');
    }
  });

  let editor = $state<{
    openGoto: () => void;
    newFile: () => void;
    openSearch: () => void;
    formatDocument: () => void;
    openStructure: () => void;
    startRename: () => void;
    startExtract: () => void;
    inlineSymbol: () => void;
  } | null>(null);
  let editorEl = $state<HTMLElement | null>(null);
  let editorScoped = $state(true);

  function onFocusIn(e: FocusEvent | PointerEvent) {
    const t = e.target as Node | null;
    editorScoped = !!(editorEl && t && editorEl.contains(t));
  }

  // While any overlay (Settings / Shortcuts / Command Palette) is open it owns
  // the keyboard — Esc (handled by the modal / palette) closes it. Only the
  // palette toggle is honoured through, so Ctrl+Shift+P also dismisses it.
  const overlayOpen = $derived(nemusStore.settingsOpen || nemusStore.shortcutsOpen || nemusStore.paletteOpen);

  function onKeyDown(e: KeyboardEvent) {
    for (const b of NEMUS_BINDINGS) {
      if (b.scope === 'editor' && !editorScoped) continue;
      if (!matchesNemus(e, b)) continue;
      if (overlayOpen && !(b.id === 'command_palette' && nemusStore.paletteOpen)) return;
      e.preventDefault();
      if (b.id === 'goto_line') editor?.openGoto();
      else if (b.id === 'new_file') editor?.newFile();
      else if (b.id === 'run_stop') void nemusEngine.toggleRun(projectStore.activeSource, projectStore.project?.path);
      else if (b.id === 'seek_to_start') void nemusEngine.seekToStart();
      else if (b.id === 'seek_to_end') void nemusEngine.seekToEnd(arrangementStore.contentEnd);
      else if (b.id === 'command_palette') nemusStore.togglePalette();
      else if (b.id === 'shortcuts') nemusStore.openShortcuts();
      else if (b.id === 'settings') nemusStore.openSettings();
      else if (b.id === 'zen') nemusStore.toggleZen();
      else if (b.id === 'find') { if (editorScoped) editor?.openSearch(); else nemusStore.requestFind(); }
      else if (b.id === 'find_usages') nemusStore.requestFindUsages();
      else if (b.id === 'format_document') editor?.formatDocument();
      else if (b.id === 'find_method') editor?.openStructure();
      else if (b.id === 'rename') editor?.startRename();
      else if (b.id === 'extract') editor?.startExtract();
      else if (b.id === 'inline') editor?.inlineSymbol();
      else if (b.id === 'new_project') projectActions.newProject();
      else if (b.id === 'open_project') projectActions.openProject();
      else if (b.id === 'open_file') projectActions.openFile();
      else if (b.id === 'save') projectActions.save();
      else if (b.id === 'render_wav') projectActions.exportWav();
      else if (b.id === 'import_audio') importActions.start();
      else if (b.id === 'commit_overrides') mixerStore.commitAll();
      return;
    }
  }

  const showLeft   = $derived(!nemusStore.zen && nemusStore.leftPanel !== null);
  const showRight  = $derived(!nemusStore.zen && nemusStore.rightPanel !== null);
  const showBottom = $derived(!nemusStore.zen && nemusStore.bottomPanel !== null);
  const showViz    = $derived(!nemusStore.collapseUi);
  const showEditor = $derived(!nemusStore.collapseTabpane);

  const leftTop = $derived<ActivityRailItem[]>([
    { id: 'files',     tooltip: 'Files',      icon: Files,    active: nemusStore.leftPanel === 'files',     onclick: () => nemusStore.toggleLeft('files') },
    { id: 'outline',   tooltip: 'Outline',    icon: ListTree, active: nemusStore.leftPanel === 'outline',   onclick: () => nemusStore.toggleLeft('outline') },
    { id: 'soundbank', tooltip: 'Sound bank', icon: Music4,   active: nemusStore.leftPanel === 'soundbank', onclick: () => nemusStore.toggleLeft('soundbank') },
  ]);
  const jobsTip = $derived(
    jobsStore.runningCount > 0
      ? `Jobs (${jobsStore.runningCount} running)`
      : 'Jobs',
  );
  const leftBottom = $derived<ActivityRailItem[]>([
    { id: 'mixer',    tooltip: 'Mixer',    icon: SlidersHorizontal, active: nemusStore.bottomPanel === 'mixer',    onclick: () => nemusStore.toggleBottom('mixer') },
    { id: 'preview',  tooltip: 'Preview',  icon: Piano,         active: nemusStore.bottomPanel === 'preview',  onclick: () => nemusStore.toggleBottom('preview') },
  ]);
  const rightTop = $derived<ActivityRailItem[]>([
    { id: 'inspector', tooltip: 'Inspector', icon: Crosshair, active: nemusStore.rightPanel === 'inspector', onclick: () => nemusStore.toggleRight('inspector') },
    { id: 'docs',      tooltip: 'Docs',      icon: BookOpen,  active: nemusStore.rightPanel === 'docs',      onclick: () => nemusStore.toggleRight('docs') },
  ]);
  // Diagnostics / system panels — toggles on the right rail (they still dock at the
  // bottom, where their wide log / list layout belongs).
  const rightBottom = $derived<ActivityRailItem[]>([
    { id: 'console',  tooltip: 'Console',  icon: Terminal,      active: nemusStore.bottomPanel === 'console',  onclick: () => nemusStore.toggleBottom('console') },
    { id: 'problems', tooltip: 'Problems', icon: AlertTriangle, active: nemusStore.bottomPanel === 'problems', onclick: () => nemusStore.toggleBottom('problems') },
    { id: 'jobs',     tooltip: jobsTip,    icon: Boxes,         active: nemusStore.bottomPanel === 'jobs',     onclick: () => nemusStore.toggleBottom('jobs') },
  ]);
</script>

<svelte:window onkeydown={onKeyDown} onfocusin={onFocusIn} onpointerdown={onFocusIn} />

{#snippet leftContent()}
  {#if nemusStore.leftPanel === 'files'}<FilesPanel />
  {:else if nemusStore.leftPanel === 'outline'}<OutlinePanel />
  {:else if nemusStore.leftPanel === 'soundbank'}<SoundBankPanel />{/if}
{/snippet}
{#snippet rightContent()}
  {#if nemusStore.rightPanel === 'inspector'}<InspectorPanel />
  {:else if nemusStore.rightPanel === 'docs'}<DocsPanel />{/if}
{/snippet}
{#snippet bottomContent()}
  {#if nemusStore.bottomPanel === 'mixer'}<MixerPanel />
  {:else if nemusStore.bottomPanel === 'preview'}<InstrumentPreviewPanel />
  {:else if nemusStore.bottomPanel === 'console'}<ConsolePanel />
  {:else if nemusStore.bottomPanel === 'problems'}<ProblemsPanel />
  {:else if nemusStore.bottomPanel === 'jobs'}<JobsPanel />{/if}
{/snippet}

{#snippet vizContent()}
  <div class="viz-wrap">
    <ArrangementView />
  </div>
{/snippet}
{#snippet editorPane()}
  <div class="editor-host" bind:this={editorEl}>
    <TabbedEditor bind:this={editor} />
  </div>
{/snippet}

<div class="shell">
  <NemusTitleBar />

  <div class="content-area">
    <WorkspaceShell showLeftRail={!nemusStore.zen} showRightRail={!nemusStore.zen}>
      {#snippet leftRail()}
        <ActivityBar side="left" ariaLabel="Navigation rail" topItems={leftTop} bottomItems={leftBottom} />
      {/snippet}
      {#snippet rightRail()}
        <ActivityBar side="right" ariaLabel="Inspection rail" topItems={rightTop} bottomItems={rightBottom} />
      {/snippet}

      {#snippet panels()}
        {#if showLeft}
          <PanelCard orientation="left" initialSize={240} minSize={170} maxSize={460}>
            {@render leftContent()}
          </PanelCard>
        {/if}

        <div class="main-col">
          <div class="body-row">
            {#if showViz && showEditor}
              <div class="card">
                <ResizablePanel direction="horizontal" initialSize={600} minSize={320} maxSize={1100}>
                  {@render vizContent()}
                </ResizablePanel>
              </div>
              <div class="card grow">{@render editorPane()}</div>
            {:else if showViz}
              <div class="card grow">{@render vizContent()}</div>
            {:else}
              <div class="card grow">{@render editorPane()}</div>
            {/if}
          </div>

          {#if showBottom}
            <PanelCard orientation="bottom" initialSize={220} minSize={90} maxSize={560}>
              {@render bottomContent()}
            </PanelCard>
          {/if}
        </div>

        {#if showRight}
          <PanelCard orientation="right" initialSize={300} minSize={210} maxSize={520}>
            {@render rightContent()}
          </PanelCard>
        {/if}
      {/snippet}
    </WorkspaceShell>
  </div>

  {#if !nemusStore.zen}
    <NemusFooter {footerExtra} />
  {/if}
</div>

<!-- Project/file pickers (New / Open / Export) — one mount for the whole window;
     menu, titlebar, and keyboard shortcuts all drive these via projectActions. -->
<NemusProjectActions />

<!-- Audio/MIDI import dialogs — one mount; driven by importActions from the
     waveform toolbar and the command palette. -->
<NemusImportActions />

<!-- Window overlays — one mount each; opened from the gear menu, the command
     palette, and the keyboard shortcuts (all via nemusStore). -->
<!-- External-change reload prompt (the open .nemus file changed on disk). -->
{#if fileWatchStore.pending}
  <ConfirmModal
    title="File changed on disk"
    message={`“${fileWatchStore.pending.name}” was modified outside nemus.`}
    detail="Reload it from disk? Any unsaved changes in the editor will be lost."
    variant="warning"
    confirmLabel="Reload"
    cancelLabel="Keep mine"
    onConfirm={() => fileWatchStore.reload()}
    onCancel={() => fileWatchStore.dismiss()}
  />
{/if}

<!-- Floating "find usages" popover (Alt+F7 / Command Palette) — one mount. -->
<UsagesPopover />

<!-- Floating "file structure" popover (Ctrl+F12 / Command Palette) — one mount. -->
<StructurePopover />

{#if nemusStore.settingsOpen}<NemusSettingsModal onClose={() => nemusStore.closeSettings()} />{/if}
{#if nemusStore.shortcutsOpen}<NemusShortcutsModal onClose={() => nemusStore.closeShortcuts()} />{/if}
{#if nemusStore.paletteOpen}<NemusCommandPalette onClose={() => nemusStore.closePalette()} />{/if}

<style>
  .shell {
    position: fixed; inset: 0;
    display: flex; flex-direction: column;
    background: var(--bg-base);
    overflow: hidden;
  }
  .content-area { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }

  /* The bg-elevated .workspace + inset .panels live in the shared <WorkspaceShell>.
     What stays here is nemus's own panel arrangement inside the panels snippet. */
  .main-col { display: flex; flex-direction: column; flex: 1; min-width: 0; overflow: hidden; gap: 4px; }
  .body-row { display: flex; flex: 1; min-width: 0; min-height: 0; overflow: hidden; gap: 4px; }

  /* Floating card: bg-base + rounded, the elevated workspace shows in the gaps. */
  .card {
    display: flex; flex-shrink: 0;
    min-width: 0; min-height: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  /* Only "grow" cards stretch their child to fill. Cards that wrap a
     ResizablePanel must NOT — the panel sizes itself and the card shrink-wraps
     to it (same as the shared PanelCard, which these viz/editor cards predate). */
  .card.grow { flex: 1; }
  .card.grow > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .viz-wrap, .editor-host {
    position: relative;
    display: flex;
    width: 100%; height: 100%;
    min-width: 0; min-height: 0;
  }
  .viz-wrap > :global(*), .editor-host > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
