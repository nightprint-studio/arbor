<script lang="ts">
  /**
   * MerulaShell — the standalone music live-coding DAW shell (Step 0, mocked).
   * Mirrors Arbor's AppShell layout language: a bg-elevated workspace with
   * floating bg-base cards inset by 4px gaps (IntelliJ feel), the icon rails
   * flush to the edges, the SplitView (read-only arrangement ↔ tab editor) over
   * the bottom panel, and the footer. Honors zen + collapse toggles.
   *
   * Reuses Arbor's shell pieces (ActivityBar, ResizablePanel, WindowControls,
   * tooltips) for consistency + zero duplication; the merula domain UI (panels,
   * arrangement, editor) lives under components/merula/.
   */
  import {
    Files, ListTree, Music4, Terminal, AlertTriangle,
    SlidersHorizontal, Crosshair, Braces, Boxes, Piano, Music2, FlaskConical, Minimize2, LayoutGrid,
  } from 'lucide-svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { tooltip } from '$lib/actions/tooltip';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import ResizablePanel from '$lib/components/layout/ResizablePanel.svelte';
  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';

  import MerulaTitleBar from './shell/MerulaTitleBar.svelte';
  import MerulaFooter from './shell/MerulaFooter.svelte';

  import FilesPanel from './panels/FilesPanel.svelte';
  import OutlinePanel from './panels/OutlinePanel.svelte';
  import SoundBankPanel from './panels/SoundBankPanel.svelte';
  import ConsolePanel from './panels/ConsolePanel.svelte';
  import ProblemsPanel from './panels/ProblemsPanel.svelte';
  import JobsPanel from './panels/JobsPanel.svelte';
  import MixerPanel from './panels/MixerPanel.svelte';
  import InspectorPanel from './panels/InspectorPanel.svelte';
  import DocsPanel from './panels/DocsPanel.svelte';
  import ScratchPanel from './panels/ScratchPanel.svelte';
  import KeyboardPanel from './panels/KeyboardPanel.svelte';
  import LauncherPanel from './panels/LauncherPanel.svelte';

  import ArrangementView from './viz/ArrangementView.svelte';
  import TabbedEditor from './editor/TabbedEditor.svelte';
  import UsagesPopover from './editor/UsagesPopover.svelte';
  import StructurePopover from './editor/StructurePopover.svelte';
  import IntentionsPopover from './editor/IntentionsPopover.svelte';

  import { onMount, onDestroy, type Snippet } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { merulaStore } from './merula-store.svelte';
  import { merulaEngine, diagnosticsStore, transportStore } from './stores/engine.svelte';
  import { levelAnalysisStore } from './stores/level-analysis.svelte';
  import { launcherStore } from './stores/launcher.svelte';
  import { transportUiStore } from './stores/transport-ui.svelte';
  import { tempoStore } from './stores/tempo.svelte';
  import { configStore } from './stores/config.svelte';
  import { packsStore } from './stores/packs.svelte';
  import { modelsStore } from './stores/models.svelte';
  import { workspaceStore } from './stores/workspace.svelte';
  import { projectStore } from './stores/project.svelte';
  import { editorSelectionStore } from './stores/editor-selection.svelte';
  import { withFileDeps } from './editor/merula-lang';
  import { fileWatchStore } from './stores/file-watch.svelte';
  import { onFsChanged } from '$lib/ipc/fs';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { projectActions } from './stores/project-actions.svelte';
  import { importActions } from './stores/import-actions.svelte';
  import { mixerStore } from './stores/mixer.svelte';
  import { referenceStore } from './stores/reference.svelte';
  import { soundsStore } from './stores/sounds.svelte';
  import { aliasesStore } from './stores/aliases.svelte';
  import { scalesStore } from './stores/scales.svelte';
  import { librariesStore } from './stores/libraries.svelte';
  import { scratchStore } from './stores/scratch.svelte';
  import { arrangementStore } from './viz/arrangement.svelte';
  import { panelSizes } from './stores/panel-sizes.svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import MerulaProjectActions from './shell/MerulaProjectActions.svelte';
  import MerulaImportActions from './shell/MerulaImportActions.svelte';
  import MerulaSettingsModal from './shell/MerulaSettingsModal.svelte';
  import MerulaShortcutsModal from './shell/MerulaShortcutsModal.svelte';
  import MerulaCommandPalette from './shell/MerulaCommandPalette.svelte';
  import RenameProjectModal from './shell/RenameProjectModal.svelte';
  import RenameFileModal from './shell/RenameFileModal.svelte';
  import WorkspacesModal from './shell/WorkspacesModal.svelte';
  import MerulaDocsPanel from './shell/MerulaDocsPanel.svelte';
  import InstrumentPreviewPanel from './preview/InstrumentPreviewPanel.svelte';
  import { MERULA_BINDINGS, matchesMerula } from './merula-keybindings';

  // Arbor-specific feedback badges (jobs · notifications) injected by the bridge
  // (MerulaWindow) and rendered in the footer's right cluster — keeps MerulaShell
  // and MerulaFooter free of Arbor store imports (extractability).
  let { footerExtra }: { footerExtra?: Snippet } = $props();

  let unEngine: UnlistenFn | null = null;
  let unPacks:  UnlistenFn | null = null;
  let unModels: UnlistenFn | null = null;
  let unFsWatch: UnlistenFn | null = null;
  let unLibs:   UnlistenFn | null = null;

  // Delete-file confirm busy flag (the modal is hosted below; the store holds the
  // target, opened from the Files sidebar or the command palette).
  let deleteFileBusy = $state(false);
  async function confirmDeleteFile() {
    const t = merulaStore.deleteFileTarget;
    if (!t || deleteFileBusy) return;
    deleteFileBusy = true;
    try { await projectStore.deleteFile(t.path); merulaStore.closeDeleteFile(); }
    finally { deleteFileBusy = false; }
  }

  onMount(async () => {
    // Live engine + sample-pack + transcription-model streams (each merula window
    // owns its listeners).
    unEngine = await merulaEngine.subscribe();
    unPacks  = await packsStore.subscribe();
    unModels = await modelsStore.subscribe();
    // Library-sync job completion (refresh + toast when a sync finishes).
    unLibs = await librariesStore.subscribe();
    // External-change detection for the open .merula file (IDE-style reload prompt).
    unFsWatch = await onFsChanged(() => void fileWatchStore.onChanged());
    void configStore.loadConfig();
    void scratchStore.restore(); // bring back the scratch tabs from the last session
    void packsStore.refresh();
    void modelsStore.refresh();
    // The DSL reference catalogue (autocomplete + hover + Docs panel). Static —
    // loaded once; failure leaves the editor working, just without language hints.
    void referenceStore.load();
    // The scale catalogue powers the scale-aware quick-fixes (snap / change-scale).
    void scalesStore.load();
    // The resolvable instrument registry powers `inst("…")` autocomplete — load
    // it up front so completions work without opening the Sound bank panel.
    void soundsStore.refresh();
    // Global sound aliases (resolved by the engine; shown in the Sound bank).
    void aliasesStore.load();
    // Restore the persisted layout, then best-effort reopen the last project.
    await workspaceStore.load();
    merulaStore.applyLayout(workspaceStore.layout);
    if (workspaceStore.lastProject) {
      projectStore.open(workspaceStore.lastProject).catch(() => {});
    }
  });

  onDestroy(() => {
    unEngine?.();
    unPacks?.();
    unModels?.();
    unFsWatch?.();
    unLibs?.();
    fileWatchStore.stop();
  });

  // Loop region enforcement (FE-driven, rides the existing seek): while playing
  // inside an enabled loop, jump back to its start when the playhead crosses the
  // end. ~30 fps transport granularity is fine for a practice/section loop.
  // `loopArmed` stops re-seeking every frame while still past the end (BE catch-up).
  let loopArmed = true;
  $effect(() => {
    const raw = transportStore.cycle; // dep (~30 fps)
    if (!transportStore.playing || !transportUiStore.loopActive) { loopArmed = true; return; }
    const loop = transportUiStore.loop!;
    const period = arrangementStore.loopCycles;
    const disp = period > 0 ? raw % period : raw;
    if (disp < loop.end - 0.05) loopArmed = true;
    else if (loopArmed) { loopArmed = false; void merulaEngine.seek(loop.start); }
  });

  // Clip launcher: drive armed launches to their quantization boundary. The store
  // fires the backend one cycle before the target line and promotes the highlight
  // when the line is crossed (absolute cycle, so the grid survives the loop).
  $effect(() => {
    launcherStore.onTransport(transportStore.cycle, transportStore.playing);
  });

  // Performance mode maximises the window (chrome already hidden by `chromeHidden`)
  // off the store flag. We use maximise — NOT OS `setFullscreen` — because the merula
  // window is decorationless (`decorations: false`), and toggling native fullscreen
  // on a frameless WebView2 window leaves the webview mis-sized on exit (a black gap
  // at the bottom). Maximise is the same path WindowControls uses and restores
  // cleanly. We only un-maximise on exit if WE maximised a previously-windowed window.
  let wasPerformance = false;
  let weMaximised = false;
  $effect(() => {
    const on = merulaStore.performance;
    if (on === wasPerformance) return;
    wasPerformance = on;
    const win = getCurrentWindow();
    if (on) {
      void win.isMaximized()
        .then((m) => { weMaximised = !m; return m ? undefined : win.maximize(); })
        .catch(() => {});
    } else if (weMaximised) {
      weMaximised = false;
      void win.unmaximize().catch(() => {});
    }
  });

  // Re-push the metronome + count-in state when playback starts: both are
  // session-only on the BE, so a toggle made while stopped (or before the device
  // opened) must be re-sent so the value survives a session re-open.
  let wasPlaying = false;
  $effect(() => {
    const p = transportStore.playing;
    if (p && !wasPlaying) {
      transportUiStore.syncMetronome();
      transportUiStore.syncCountIn();
      mixerStore.syncMaster(); // master gain + reverb are session-only — re-apply on play

      // Re-apply any launched clips: Play stages the base tracks, which would
      // otherwise clobber a selection armed while stopped.
      launcherStore.resync();
    } else if (!p && wasPlaying) {
      // One transport Stop clears the launcher too (cells go idle, next play =
      // base) — no second click in the grid. The grid's own Stop is the soft
      // "clips off, keep playing" action.
      launcherStore.onStop();
    }
    wasPlaying = p;
  });

  // On project open/switch: load the declared libraries and auto-sync any that
  // aren't present yet (the user opted into fetch-if-missing on open).
  $effect(() => {
    const path = projectStore.project?.path;
    if (!path) return;
    // Restore the per-project master mix (master gain + reverb decay — neither has
    // a .merula representation, so without this they'd reset on every reopen).
    void mixerStore.loadMix(path);
    void librariesStore.refresh(path).then(() => {
      if (librariesStore.missing > 0) void librariesStore.sync();
    });
  });

  // Watch the directory of the active file for external edits (re-armed on tab
  // switch / cross-file open).
  $effect(() => {
    void projectStore.activeFilePath;
    void fileWatchStore.watchActive();
  });

  // Keep the arrangement query (loop period + tempo) fresh after every eval —
  // centralised here (always mounted) so the footer's render estimate updates on
  // edit / file switch even when no viz panel is open to drive the query. The
  // store coalesces calls (debounced single timer), so this is redundant-safe
  // with the panels that also schedule it.
  $effect(() => {
    void diagnosticsStore.errors; // dep: a fresh array is set on each eval
    arrangementStore.schedule();
    // The source's tempo is authoritative again after an eval — drop any live
    // tap-tempo / nudge override (mirrors the mixer's gain/pan rebaseline).
    tempoStore.reset();
    // An edit invalidates the offline level-analysis snapshot (its clip windows
    // were measured against the old source) — clear the LEDs / underlines so a
    // stale result never lingers; the user re-runs "Check levels" when ready.
    levelAnalysisStore.clear();
  });

  // Refresh the clip-launcher grid only while its panel is open — `scene(...)`
  // declarations change with the source, but querying them on every eval for
  // everyone who isn't launching clips is wasted IPC (it ran on every load/edit).
  // Loads when the launcher opens and after each eval while it's open.
  $effect(() => {
    void diagnosticsStore.errors; // dep: re-fetch on each eval
    if (merulaStore.bottomPanel === 'launcher') void launcherStore.load();
  });

  // Mirror layout changes to the persisted window state (debounced in the
  // store). Read the snapshot inside the effect so it tracks the panel state.
  $effect(() => {
    const snap = merulaStore.layoutSnapshot();
    workspaceStore.persistLayout(snap);
  });

  // Bridge the shared Jobs overlay's "View output" button into merula. That button
  // (in the shared JobsOverlay, mounted here via the footer badge) targets the
  // main-app bottom-panel system (`uiStore.activeBottomSection`), which the merula
  // window doesn't use — so it appeared to do nothing. Watch that one-shot signal
  // and open merula's own Jobs panel instead; the overlay already set the active
  // job on the shared `jobsStore`, so the panel drills straight into its output.
  $effect(() => {
    if (uiStore.activeBottomSection === 'jobs') {
      uiStore.setActiveBottomSection(null); // consume the signal (merula ignores it otherwise)
      merulaStore.showBottom('jobs');
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
    showIntentions: () => void;
  } | null>(null);
  let editorEl = $state<HTMLElement | null>(null);
  let editorScoped = $state(true);

  function onFocusIn(e: FocusEvent | PointerEvent) {
    const t = e.target as Node | null;
    editorScoped = !!(editorEl && t && editorEl.contains(t));
  }

  // While any overlay (Settings / Shortcuts / Command Palette) is open it owns
  // the keyboard — Esc (handled by the modal / palette) closes it. Only the
  // palette toggle is honoured through, so Ctrl+K also dismisses it.
  const overlayOpen = $derived(merulaStore.settingsOpen || merulaStore.shortcutsOpen || merulaStore.paletteOpen || merulaStore.renameProjectOpen || merulaStore.docsOpen);

  // Play the editor selection one-shot, or the whole active file when nothing is
  // selected — isolated from the song transport (audition bus). A selection is
  // resolved against the file's preamble (withFileDeps) so a bare variable plays.
  async function playSelectionOrFile() {
    const file = projectStore.activeSource;
    const r = editorSelectionStore.primary;
    const src = r ? await withFileDeps(file, file.slice(r.from, r.to)) : file;
    void merulaEngine.playSnippet(src, projectStore.project?.path);
  }

  function onKeyDown(e: KeyboardEvent) {
    // Esc clears the loop region. Global (the loop is global state) but yields Esc
    // to the editor (CodeMirror owns it for autocomplete / multi-cursor) and to any
    // open overlay (which closes on Esc). Only fires when a loop actually exists, so
    // it never swallows a plain Esc otherwise.
    if (e.key === 'Escape' && !overlayOpen && !editorScoped && transportUiStore.loop) {
      transportUiStore.clearLoop();
      e.preventDefault();
      return;
    }
    for (const b of MERULA_BINDINGS) {
      if (b.scope === 'editor' && !editorScoped) continue;
      if (!matchesMerula(e, b)) continue;
      if (overlayOpen && !((b.id === 'command_palette' && merulaStore.paletteOpen) || (b.id === 'docs' && merulaStore.docsOpen))) return;
      e.preventDefault();
      if (b.id === 'goto_line') editor?.openGoto();
      else if (b.id === 'new_file') editor?.newFile();
      else if (b.id === 'run_stop') void merulaEngine.toggleRun(projectStore.activeSource, projectStore.project?.path);
      else if (b.id === 'play_selection') playSelectionOrFile();
      else if (b.id === 'toggle_scratch') merulaStore.toggleBottom('scratch');
      else if (b.id === 'toggle_launcher') merulaStore.toggleBottom('launcher');
      else if (b.id === 'seek_to_start') { transportUiStore.setCursor(0); void merulaEngine.seekToStart(); }
      else if (b.id === 'seek_to_end') { transportUiStore.setCursor(arrangementStore.contentEnd); void merulaEngine.seekToEnd(arrangementStore.contentEnd); }
      else if (b.id === 'step_back') transportUiStore.stepBy(-configStore.skipStep, arrangementStore.contentEnd);
      else if (b.id === 'step_fwd')  transportUiStore.stepBy(configStore.skipStep, arrangementStore.contentEnd);
      else if (b.id === 'play_from_cursor') transportUiStore.playFromCursor();
      else if (b.id === 'toggle_loop') transportUiStore.toggleLoop();
      else if (b.id === 'add_marker') transportUiStore.addMarker(transportUiStore.cursor);
      else if (b.id === 'toggle_metronome') transportUiStore.toggleMetronome();
      else if (b.id === 'cycle_count_in') transportUiStore.cycleCountIn();
      else if (b.id === 'command_palette') merulaStore.togglePalette();
      else if (b.id === 'docs') merulaStore.toggleDocs();
      else if (b.id === 'shortcuts') merulaStore.openShortcuts();
      else if (b.id === 'settings') merulaStore.openSettings();
      else if (b.id === 'zen') merulaStore.toggleZen();
      else if (b.id === 'performance') merulaStore.togglePerformance();
      else if (b.id === 'find') { if (editorScoped) editor?.openSearch(); else merulaStore.requestFind(); }
      else if (b.id === 'find_usages') merulaStore.requestFindUsages();
      else if (b.id === 'format_document') editor?.formatDocument();
      else if (b.id === 'find_method') editor?.openStructure();
      else if (b.id === 'rename') editor?.startRename();
      else if (b.id === 'extract') editor?.startExtract();
      else if (b.id === 'inline') editor?.inlineSymbol();
      else if (b.id === 'intentions') editor?.showIntentions();
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

  const showLeft   = $derived(!merulaStore.chromeHidden && merulaStore.leftPanel !== null);
  const showRight  = $derived(!merulaStore.chromeHidden && merulaStore.rightPanel !== null);
  const showBottom = $derived(!merulaStore.chromeHidden && merulaStore.bottomPanel !== null);
  const showViz    = $derived(!merulaStore.collapseUi);
  const showEditor = $derived(!merulaStore.collapseTabpane);

  const leftTop = $derived<ActivityRailItem[]>([
    { id: 'files',     tooltip: 'Files',      icon: Files,    active: merulaStore.leftPanel === 'files',     onclick: () => merulaStore.toggleLeft('files') },
    { id: 'outline',   tooltip: 'Outline',    icon: ListTree, active: merulaStore.leftPanel === 'outline',   onclick: () => merulaStore.toggleLeft('outline') },
    { id: 'soundbank', tooltip: 'Sound bank', icon: Music4,   active: merulaStore.leftPanel === 'soundbank', onclick: () => merulaStore.toggleLeft('soundbank') },
  ]);
  const jobsTip = $derived(
    jobsStore.runningCount > 0
      ? `Jobs (${jobsStore.runningCount} running)`
      : 'Jobs',
  );
  const leftBottom = $derived<ActivityRailItem[]>([
    { id: 'mixer',    tooltip: 'Mixer',    icon: SlidersHorizontal, active: merulaStore.bottomPanel === 'mixer',    onclick: () => merulaStore.toggleBottom('mixer') },
    { id: 'preview',  tooltip: 'Preview',  icon: Music2,        active: merulaStore.bottomPanel === 'preview',  onclick: () => merulaStore.toggleBottom('preview') },
    { id: 'keyboard', tooltip: 'Keyboard (live notes)', icon: Piano, active: merulaStore.bottomPanel === 'keyboard', onclick: () => merulaStore.toggleBottom('keyboard') },
    { id: 'launcher', tooltip: 'Clip launcher', shortcut: 'Ctrl+Shift+G', icon: LayoutGrid, active: merulaStore.bottomPanel === 'launcher', onclick: () => merulaStore.toggleBottom('launcher') },
    { id: 'scratch',  tooltip: 'Scratch', shortcut: 'Ctrl+Shift+S', icon: FlaskConical, active: merulaStore.bottomPanel === 'scratch', onclick: () => merulaStore.toggleBottom('scratch') },
  ]);
  const rightTop = $derived<ActivityRailItem[]>([
    { id: 'inspector', tooltip: 'Inspector', icon: Crosshair, active: merulaStore.rightPanel === 'inspector', onclick: () => merulaStore.toggleRight('inspector') },
    { id: 'docs',      tooltip: 'Language reference', icon: Braces, active: merulaStore.rightPanel === 'docs', onclick: () => merulaStore.toggleRight('docs') },
  ]);
  // Diagnostics / system panels — toggles on the right rail (they still dock at the
  // bottom, where their wide log / list layout belongs).
  const rightBottom = $derived<ActivityRailItem[]>([
    { id: 'console',  tooltip: 'Console',  icon: Terminal,      active: merulaStore.bottomPanel === 'console',  onclick: () => merulaStore.toggleBottom('console') },
    { id: 'problems', tooltip: 'Problems', icon: AlertTriangle, active: merulaStore.bottomPanel === 'problems', onclick: () => merulaStore.toggleBottom('problems') },
    { id: 'jobs',     tooltip: jobsTip,    icon: Boxes,         active: merulaStore.bottomPanel === 'jobs',     onclick: () => merulaStore.toggleBottom('jobs') },
  ]);
</script>

<svelte:window onkeydown={onKeyDown} onfocusin={onFocusIn} onpointerdown={onFocusIn} />

{#snippet leftContent()}
  {#if merulaStore.leftPanel === 'files'}<FilesPanel />
  {:else if merulaStore.leftPanel === 'outline'}<OutlinePanel />
  {:else if merulaStore.leftPanel === 'soundbank'}<SoundBankPanel />{/if}
{/snippet}
{#snippet rightContent()}
  {#if merulaStore.rightPanel === 'inspector'}<InspectorPanel />
  {:else if merulaStore.rightPanel === 'docs'}<DocsPanel />{/if}
{/snippet}
{#snippet bottomContent()}
  {#if merulaStore.bottomPanel === 'mixer'}<MixerPanel />
  {:else if merulaStore.bottomPanel === 'preview'}<InstrumentPreviewPanel />
  {:else if merulaStore.bottomPanel === 'keyboard'}<KeyboardPanel />
  {:else if merulaStore.bottomPanel === 'launcher'}<LauncherPanel />
  {:else if merulaStore.bottomPanel === 'console'}<ConsolePanel />
  {:else if merulaStore.bottomPanel === 'problems'}<ProblemsPanel />
  {:else if merulaStore.bottomPanel === 'scratch'}<ScratchPanel />
  {:else if merulaStore.bottomPanel === 'jobs'}<JobsPanel />{/if}
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
  {#if !merulaStore.performance}<MerulaTitleBar />{/if}

  <div class="content-area">
    <WorkspaceShell showLeftRail={!merulaStore.chromeHidden} showRightRail={!merulaStore.chromeHidden}>
      {#snippet leftRail()}
        <ActivityBar side="left" ariaLabel="Navigation rail" topItems={leftTop} bottomItems={leftBottom} />
      {/snippet}
      {#snippet rightRail()}
        <ActivityBar side="right" ariaLabel="Inspection rail" topItems={rightTop} bottomItems={rightBottom} />
      {/snippet}

      {#snippet panels()}
        {#if showLeft}
          <PanelCard orientation="left" initialSize={panelSizes.left} minSize={170} maxSize={460}
            onResize={(px) => panelSizes.setLeft(px)}>
            {@render leftContent()}
          </PanelCard>
        {/if}

        <div class="main-col">
          <div class="body-row">
            {#if showViz && showEditor}
              <div class="card">
                <ResizablePanel direction="horizontal" initialSize={panelSizes.viz} minSize={320} maxSize={1100}
                  onResize={(px) => panelSizes.setViz(px)}>
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
            <PanelCard orientation="bottom" initialSize={panelSizes.bottom} minSize={90} maxSize={560}
              onResize={(px) => panelSizes.setBottom(px)}>
              {@render bottomContent()}
            </PanelCard>
          {/if}
        </div>

        {#if showRight}
          <PanelCard orientation="right" initialSize={panelSizes.right} minSize={210} maxSize={520}
            onResize={(px) => panelSizes.setRight(px)}>
            {@render rightContent()}
          </PanelCard>
        {/if}
      {/snippet}
    </WorkspaceShell>
  </div>

  {#if !merulaStore.chromeHidden}
    <MerulaFooter {footerExtra} />
  {/if}

  <!-- Performance mode: a single floating affordance so the full-screen stage is
       never a trap (F11 also exits). Shows the live play state at a glance. -->
  {#if merulaStore.performance}
    <button
      class="perf-exit"
      type="button"
      aria-label="Exit performance mode"
      use:tooltip={{ content: 'Exit performance mode', shortcut: 'F11', description: 'Leave full-screen and bring the chrome back' }}
      onclick={() => merulaStore.setPerformance(false)}
    >
      <Minimize2 size={14} />
      <span>Exit</span>
    </button>
  {/if}
</div>

<!-- Project/file pickers (New / Open / Export) — one mount for the whole window;
     menu, titlebar, and keyboard shortcuts all drive these via projectActions. -->
<MerulaProjectActions />

<!-- Audio/MIDI import dialogs — one mount; driven by importActions from the
     waveform toolbar and the command palette. -->
<MerulaImportActions />

<!-- Window overlays — one mount each; opened from the gear menu, the command
     palette, and the keyboard shortcuts (all via merulaStore). -->
<!-- External-change reload prompt (the open .merula file changed on disk). -->
{#if fileWatchStore.pending}
  <ConfirmModal
    title="File changed on disk"
    message={`“${fileWatchStore.pending.name}” was modified outside merula.`}
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

<!-- Floating "context actions" popover (Alt+Enter / Command Palette) — one mount. -->
<IntentionsPopover />

{#if merulaStore.settingsOpen}<MerulaSettingsModal onClose={() => merulaStore.closeSettings()} />{/if}
{#if merulaStore.shortcutsOpen}<MerulaShortcutsModal onClose={() => merulaStore.closeShortcuts()} />{/if}
{#if merulaStore.paletteOpen}<MerulaCommandPalette onClose={() => merulaStore.closePalette()} />{/if}
{#if merulaStore.renameProjectOpen}<RenameProjectModal onClose={() => merulaStore.closeRenameProject()} />{/if}
{#if merulaStore.workspacesOpen}<WorkspacesModal onClose={() => merulaStore.closeWorkspaces()} />{/if}
{#if merulaStore.renameFileTarget}
  <RenameFileModal
    path={merulaStore.renameFileTarget.path}
    currentName={merulaStore.renameFileTarget.name}
    onClose={() => merulaStore.closeRenameFile()}
  />
{/if}
{#if merulaStore.deleteFileTarget}
  <ConfirmModal
    title="Delete file"
    message={`Move “${merulaStore.deleteFileTarget.name}” to the Recycle Bin?`}
    detail="The file is recoverable from the OS trash."
    variant="danger"
    confirmLabel="Delete"
    busy={deleteFileBusy}
    onConfirm={confirmDeleteFile}
    onCancel={() => merulaStore.closeDeleteFile()}
  />
{/if}
{#if merulaStore.docsOpen}<MerulaDocsPanel onClose={() => merulaStore.closeDocs()} />{/if}

<style>
  .shell {
    position: fixed; inset: 0;
    display: flex; flex-direction: column;
    background: var(--bg-base);
    overflow: hidden;
  }
  .content-area { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }

  /* The bg-elevated .workspace + inset .panels live in the shared <WorkspaceShell>.
     What stays here is merula's own panel arrangement inside the panels snippet. */
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

  /* Performance mode: chrome is gone; only this floating exit remains, tucked in
     the top-right so it never competes with the stage. Fades to near-invisible
     until hovered/focused so it doesn't draw the eye during play. */
  .perf-exit {
    position: fixed;
    top: 8px; right: 8px;
    z-index: 50;
    display: inline-flex; align-items: center; gap: 5px;
    height: 26px; padding: 0 9px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    font-size: 11px; font-weight: 600;
    cursor: pointer;
    opacity: 0.25;
    transition: opacity var(--transition-fast), background var(--transition-fast), color var(--transition-fast);
  }
  .perf-exit:hover { opacity: 1; background: var(--bg-hover); color: var(--text-primary); }
  .perf-exit:focus-visible {
    opacity: 1; outline: none;
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 55%, transparent);
  }
</style>
