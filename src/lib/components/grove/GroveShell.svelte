<script lang="ts">
  /**
   * GroveShell — the standalone music live-coding DAW shell (Step 0, mocked).
   * Mirrors Arbor's AppShell layout language: a bg-elevated workspace with
   * floating bg-base cards inset by 4px gaps (IntelliJ feel), the icon rails
   * flush to the edges, the SplitView (read-only arrangement ↔ tab editor) over
   * the bottom panel, and the footer. Honors zen + collapse toggles.
   *
   * Reuses Arbor's shell pieces (ActivityBar, ResizablePanel, WindowControls,
   * tooltips) for consistency + zero duplication; the grove domain UI (panels,
   * arrangement, editor) lives under components/grove/.
   */
  import {
    Files, ListTree, Music4, Terminal, AlertTriangle,
    SlidersHorizontal, Crosshair, BookOpen, Boxes,
  } from 'lucide-svelte';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import ResizablePanel from '$lib/components/layout/ResizablePanel.svelte';
  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';

  import GroveTitleBar from './shell/GroveTitleBar.svelte';
  import GroveFooter from './shell/GroveFooter.svelte';

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

  import { onMount, onDestroy, type Snippet } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { groveStore } from './grove-store.svelte';
  import { groveEngine } from './stores/engine.svelte';
  import { configStore } from './stores/config.svelte';
  import { packsStore } from './stores/packs.svelte';
  import { workspaceStore } from './stores/workspace.svelte';
  import { projectStore } from './stores/project.svelte';
  import { projectActions } from './stores/project-actions.svelte';
  import { mixerStore } from './stores/mixer.svelte';
  import { referenceStore } from './stores/reference.svelte';
  import { soundsStore } from './stores/sounds.svelte';
  import { arrangementStore } from './viz/arrangement.svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import GroveProjectActions from './shell/GroveProjectActions.svelte';
  import GroveSettingsModal from './shell/GroveSettingsModal.svelte';
  import GroveShortcutsModal from './shell/GroveShortcutsModal.svelte';
  import GroveCommandPalette from './shell/GroveCommandPalette.svelte';
  import { GROVE_BINDINGS, matchesGrove } from './grove-keybindings';

  // Arbor-specific feedback badges (jobs · notifications) injected by the bridge
  // (GroveWindow) and rendered in the footer's right cluster — keeps GroveShell
  // and GroveFooter free of Arbor store imports (extractability).
  let { footerExtra }: { footerExtra?: Snippet } = $props();

  let unEngine: UnlistenFn | null = null;
  let unPacks:  UnlistenFn | null = null;

  onMount(async () => {
    // Live engine + sample-pack streams (each grove window owns its listeners).
    unEngine = await groveEngine.subscribe();
    unPacks  = await packsStore.subscribe();
    void configStore.loadConfig();
    void packsStore.refresh();
    // The DSL reference catalogue (autocomplete + hover + Docs panel). Static —
    // loaded once; failure leaves the editor working, just without language hints.
    void referenceStore.load();
    // The resolvable instrument registry powers `inst("…")` autocomplete — load
    // it up front so completions work without opening the Sound bank panel.
    void soundsStore.refresh();
    // Restore the persisted layout, then best-effort reopen the last project.
    await workspaceStore.load();
    groveStore.applyLayout(workspaceStore.layout);
    if (workspaceStore.lastProject) {
      projectStore.open(workspaceStore.lastProject).catch(() => {});
    }
  });

  onDestroy(() => {
    unEngine?.();
    unPacks?.();
  });

  // Mirror layout changes to the persisted window state (debounced in the
  // store). Read the snapshot inside the effect so it tracks the panel state.
  $effect(() => {
    const snap = groveStore.layoutSnapshot();
    workspaceStore.persistLayout(snap);
  });

  // Bridge the shared Jobs overlay's "View output" button into grove. That button
  // (in the shared JobsOverlay, mounted here via the footer badge) targets the
  // main-app bottom-panel system (`uiStore.activeBottomSection`), which the grove
  // window doesn't use — so it appeared to do nothing. Watch that one-shot signal
  // and open grove's own Jobs panel instead; the overlay already set the active
  // job on the shared `jobsStore`, so the panel drills straight into its output.
  $effect(() => {
    if (uiStore.activeBottomSection === 'jobs') {
      uiStore.setActiveBottomSection(null); // consume the signal (grove ignores it otherwise)
      groveStore.showBottom('jobs');
    }
  });

  let editor = $state<{ openGoto: () => void; newFile: () => void; openSearch: () => void } | null>(null);
  let editorEl = $state<HTMLElement | null>(null);
  let editorScoped = $state(true);

  function onFocusIn(e: FocusEvent | PointerEvent) {
    const t = e.target as Node | null;
    editorScoped = !!(editorEl && t && editorEl.contains(t));
  }

  // While any overlay (Settings / Shortcuts / Command Palette) is open it owns
  // the keyboard — Esc (handled by the modal / palette) closes it. Only the
  // palette toggle is honoured through, so Ctrl+Shift+P also dismisses it.
  const overlayOpen = $derived(groveStore.settingsOpen || groveStore.shortcutsOpen || groveStore.paletteOpen);

  function onKeyDown(e: KeyboardEvent) {
    for (const b of GROVE_BINDINGS) {
      if (b.scope === 'editor' && !editorScoped) continue;
      if (!matchesGrove(e, b)) continue;
      if (overlayOpen && !(b.id === 'command_palette' && groveStore.paletteOpen)) return;
      e.preventDefault();
      if (b.id === 'goto_line') editor?.openGoto();
      else if (b.id === 'new_file') editor?.newFile();
      else if (b.id === 'run_stop') void groveEngine.toggleRun(projectStore.activeSource, projectStore.project?.path);
      else if (b.id === 'seek_to_start') void groveEngine.seekToStart();
      else if (b.id === 'seek_to_end') void groveEngine.seekToEnd(arrangementStore.contentEnd);
      else if (b.id === 'command_palette') groveStore.togglePalette();
      else if (b.id === 'shortcuts') groveStore.openShortcuts();
      else if (b.id === 'settings') groveStore.openSettings();
      else if (b.id === 'zen') groveStore.toggleZen();
      else if (b.id === 'find') { if (editorScoped) editor?.openSearch(); else groveStore.requestFind(); }
      else if (b.id === 'new_project') projectActions.newProject();
      else if (b.id === 'open_project') projectActions.openProject();
      else if (b.id === 'open_file') projectActions.openFile();
      else if (b.id === 'save') projectActions.save();
      else if (b.id === 'render_wav') projectActions.exportWav();
      else if (b.id === 'commit_overrides') mixerStore.commitAll();
      return;
    }
  }

  const showLeft   = $derived(!groveStore.zen && groveStore.leftPanel !== null);
  const showRight  = $derived(!groveStore.zen && groveStore.rightPanel !== null);
  const showBottom = $derived(!groveStore.zen && groveStore.bottomPanel !== null);
  const showViz    = $derived(!groveStore.collapseUi);
  const showEditor = $derived(!groveStore.collapseTabpane);

  const leftTop = $derived<ActivityRailItem[]>([
    { id: 'files',     tooltip: 'Files',      icon: Files,    active: groveStore.leftPanel === 'files',     onclick: () => groveStore.toggleLeft('files') },
    { id: 'outline',   tooltip: 'Outline',    icon: ListTree, active: groveStore.leftPanel === 'outline',   onclick: () => groveStore.toggleLeft('outline') },
    { id: 'soundbank', tooltip: 'Sound bank', icon: Music4,   active: groveStore.leftPanel === 'soundbank', onclick: () => groveStore.toggleLeft('soundbank') },
  ]);
  const jobsTip = $derived(
    jobsStore.runningCount > 0
      ? `Jobs (${jobsStore.runningCount} running)`
      : 'Jobs',
  );
  const leftBottom = $derived<ActivityRailItem[]>([
    { id: 'mixer',    tooltip: 'Mixer',    icon: SlidersHorizontal, active: groveStore.bottomPanel === 'mixer',    onclick: () => groveStore.toggleBottom('mixer') },
    { id: 'console',  tooltip: 'Console',  icon: Terminal,      active: groveStore.bottomPanel === 'console',  onclick: () => groveStore.toggleBottom('console') },
    { id: 'problems', tooltip: 'Problems', icon: AlertTriangle, active: groveStore.bottomPanel === 'problems', onclick: () => groveStore.toggleBottom('problems') },
    { id: 'jobs',     tooltip: jobsTip,    icon: Boxes,         active: groveStore.bottomPanel === 'jobs',     onclick: () => groveStore.toggleBottom('jobs') },
  ]);
  const rightTop = $derived<ActivityRailItem[]>([
    { id: 'inspector', tooltip: 'Inspector', icon: Crosshair, active: groveStore.rightPanel === 'inspector', onclick: () => groveStore.toggleRight('inspector') },
    { id: 'docs',      tooltip: 'Docs',      icon: BookOpen,  active: groveStore.rightPanel === 'docs',      onclick: () => groveStore.toggleRight('docs') },
  ]);
</script>

<svelte:window onkeydown={onKeyDown} onfocusin={onFocusIn} onpointerdown={onFocusIn} />

{#snippet leftContent()}
  {#if groveStore.leftPanel === 'files'}<FilesPanel />
  {:else if groveStore.leftPanel === 'outline'}<OutlinePanel />
  {:else if groveStore.leftPanel === 'soundbank'}<SoundBankPanel />{/if}
{/snippet}
{#snippet rightContent()}
  {#if groveStore.rightPanel === 'inspector'}<InspectorPanel />
  {:else if groveStore.rightPanel === 'docs'}<DocsPanel />{/if}
{/snippet}
{#snippet bottomContent()}
  {#if groveStore.bottomPanel === 'mixer'}<MixerPanel />
  {:else if groveStore.bottomPanel === 'console'}<ConsolePanel />
  {:else if groveStore.bottomPanel === 'problems'}<ProblemsPanel />
  {:else if groveStore.bottomPanel === 'jobs'}<JobsPanel />{/if}
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
  <GroveTitleBar />

  <div class="content-area">
    <WorkspaceShell showLeftRail={!groveStore.zen} showRightRail={!groveStore.zen}>
      {#snippet leftRail()}
        <ActivityBar side="left" ariaLabel="Navigation rail" topItems={leftTop} bottomItems={leftBottom} />
      {/snippet}
      {#snippet rightRail()}
        <ActivityBar side="right" ariaLabel="Inspection rail" topItems={rightTop} />
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

  {#if !groveStore.zen}
    <GroveFooter {footerExtra} />
  {/if}
</div>

<!-- Project/file pickers (New / Open / Export) — one mount for the whole window;
     menu, titlebar, and keyboard shortcuts all drive these via projectActions. -->
<GroveProjectActions />

<!-- Window overlays — one mount each; opened from the gear menu, the command
     palette, and the keyboard shortcuts (all via groveStore). -->
{#if groveStore.settingsOpen}<GroveSettingsModal onClose={() => groveStore.closeSettings()} />{/if}
{#if groveStore.shortcutsOpen}<GroveShortcutsModal onClose={() => groveStore.closeShortcuts()} />{/if}
{#if groveStore.paletteOpen}<GroveCommandPalette onClose={() => groveStore.closePalette()} />{/if}

<style>
  .shell {
    position: fixed; inset: 0;
    display: flex; flex-direction: column;
    background: var(--bg-base);
    overflow: hidden;
  }
  .content-area { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }

  /* The bg-elevated .workspace + inset .panels live in the shared <WorkspaceShell>.
     What stays here is grove's own panel arrangement inside the panels snippet. */
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
