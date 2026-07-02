<script lang="ts">
  /**
   * TytoShell — the screen-recorder control panel (mocked).
   *
   * Mirrors Arbor's IntelliJ layout language: a bg-elevated workspace with
   * floating bg-base cards inset by 4px gaps. The capture surface (source,
   * options) is the hero card; the captures library docks on the right and
   * collapses via the right rail. The primary action + mode switch live in the
   * titlebar. Reuses the shared chrome (TitleBar, WorkspaceShell, PanelCard,
   * ActivityBar, WindowControls) so Tyto reads as a first-class Arbor product.
   *
   * Every action here is reachable from the keyboard — see `onKeyDown` and the
   * canonical list in `tyto-shortcuts.ts`.
   */
  import { Images } from 'lucide-svelte';
  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import TytoTitleBar from './shell/TytoTitleBar.svelte';
  import TytoFooter from './shell/TytoFooter.svelte';
  import CapturePanel from './panels/CapturePanel.svelte';
  import RecordingsPanel from './panels/RecordingsPanel.svelte';
  import TytoSettingsModal from './TytoSettingsModal.svelte';
  import TytoShortcutsModal from './TytoShortcutsModal.svelte';
  import TytoAboutModal from './TytoAboutModal.svelte';
  import TytoDocsPanel from './TytoDocsPanel.svelte';
  import TytoMiniBar from './TytoMiniBar.svelte';
  import { recorderStore, type TargetKind } from '$lib/stores/tyto/recorder.svelte';
  import { tytoUiStore } from '$lib/stores/tyto/ui.svelte';

  let libraryWidth = $state(360);

  // After any capture completes the library must surface it — reveal the right rail
  // whenever a fresh capture lands (the store bumps `captureSignal`). This is also
  // what returns the mini toolbar to the full window: "capture from the reduced bar →
  // the normal one opens".
  let lastCaptureSignal = 0;
  $effect(() => {
    const sig = recorderStore.captureSignal;
    if (sig !== lastCaptureSignal) {
      lastCaptureSignal = sig;
      if (sig > 0) {
        tytoUiStore.setLibraryOpen(true);
        if (tytoUiStore.compact) tytoUiStore.setCompact(false);
      }
    }
  });

  const railItems = $derived<ActivityRailItem[]>([
    {
      id: 'library',
      tooltip: { content: 'Captures library', shortcut: 'Ctrl+Shift+B' },
      icon: Images,
      active: tytoUiStore.libraryOpen,
      onclick: () => tytoUiStore.toggleLibrary(),
    },
  ]);

  function cycleSource() {
    const order: TargetKind[] = ['monitor', 'window', 'region'];
    const next = order[(order.indexOf(recorderStore.targetKind) + 1) % order.length];
    recorderStore.setTargetKind(next);
  }

  function primaryCapture() {
    if (recorderStore.mode === 'record') {
      if (recorderStore.recording) recorderStore.stopRecording();
      else recorderStore.startRecording();
    } else {
      recorderStore.takeScreenshot();
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    // Help toggles — available even with a panel open (F1 toggles docs closed).
    if (e.key === 'F1' && !e.shiftKey) { tytoUiStore.toggleDocs(); e.preventDefault(); return; }
    if (e.key === 'F1' && e.shiftKey)  { tytoUiStore.openShortcuts(); e.preventDefault(); return; }

    // Behind a modal, let the dialog own the keyboard (its own Esc, Tab, …).
    if (tytoUiStore.anyModalOpen) return;

    // Compact mini toolbar: only the capture essentials + expand are live (the
    // full-window features have no surface here).
    if (tytoUiStore.compact) {
      if (mod && e.shiftKey && key === 'c')          { tytoUiStore.setCompact(false); e.preventDefault(); return; }
      if (mod && key === 'enter')                    { primaryCapture(); e.preventDefault(); return; }
      if (mod && !e.shiftKey && key === '1')         { recorderStore.setMode('record'); e.preventDefault(); return; }
      if (mod && !e.shiftKey && key === '2')         { recorderStore.setMode('screenshot'); e.preventDefault(); return; }
      if (mod && e.shiftKey && e.code === 'Digit1')  { recorderStore.setTargetKind('monitor'); e.preventDefault(); return; }
      if (mod && e.shiftKey && e.code === 'Digit2')  { recorderStore.setTargetKind('window'); e.preventDefault(); return; }
      if (mod && e.shiftKey && e.code === 'Digit3')  { void recorderStore.openScreenRegion(); e.preventDefault(); return; }
      return;
    }

    if (mod && e.shiftKey && key === 'c')      { tytoUiStore.setCompact(true); e.preventDefault(); return; }
    if (mod && !e.shiftKey && key === ',')     { tytoUiStore.openSettings(); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'b')      { tytoUiStore.toggleLibrary(); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'o')      { void recorderStore.revealOutputFolder(); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 's')      { cycleSource(); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'd')      { void recorderStore.openScreenRegion(); e.preventDefault(); return; }
    // Direct source select — e.code so Shift+digit is layout-robust (Shift+1 = '!' on
    // some layouts), and distinct from the shift-less Ctrl+1/2 mode switches.
    if (mod && e.shiftKey && e.code === 'Digit1') { recorderStore.setTargetKind('monitor'); e.preventDefault(); return; }
    if (mod && e.shiftKey && e.code === 'Digit2') { recorderStore.setTargetKind('window');  e.preventDefault(); return; }
    if (mod && e.shiftKey && e.code === 'Digit3') { recorderStore.setTargetKind('region');   e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'a')      { if (recorderStore.mode === 'record') recorderStore.toggleSystemAudio(); e.preventDefault(); return; }
    if (mod && !e.shiftKey && key === '1')     { recorderStore.setMode('record'); e.preventDefault(); return; }
    if (mod && !e.shiftKey && key === '2')     { recorderStore.setMode('screenshot'); e.preventDefault(); return; }
    if (mod && key === 'enter')                { primaryCapture(); e.preventDefault(); return; }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if tytoUiStore.compact}
  <TytoMiniBar />
{:else}
<div class="shell">
  <TytoTitleBar />

  <div class="content-area">
    <WorkspaceShell showRightRail>
      {#snippet rightRail()}
        <ActivityBar side="right" ariaLabel="Tyto rail" topItems={railItems} />
      {/snippet}
      {#snippet panels()}
        <div class="main-col">
          <div class="card grow"><CapturePanel /></div>
        </div>
        {#if tytoUiStore.libraryOpen}
          <PanelCard
            orientation="right"
            initialSize={libraryWidth}
            minSize={300}
            maxSize={480}
            onResize={(px) => (libraryWidth = px)}
          >
            <RecordingsPanel />
          </PanelCard>
        {/if}
      {/snippet}
    </WorkspaceShell>
  </div>

  <TytoFooter />
</div>

{#if tytoUiStore.settingsOpen}
  <TytoSettingsModal onClose={() => tytoUiStore.closeSettings()} />
{/if}
{#if tytoUiStore.shortcutsOpen}
  <TytoShortcutsModal onClose={() => tytoUiStore.closeShortcuts()} />
{/if}
{#if tytoUiStore.aboutOpen}
  <TytoAboutModal onClose={() => tytoUiStore.closeAbout()} />
{/if}
{#if tytoUiStore.docsOpen}
  <TytoDocsPanel onClose={() => tytoUiStore.closeDocs()} />
{/if}
{/if}

<style>
  .shell {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
    overflow: hidden;
  }
  .content-area { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }

  .main-col { display: flex; flex-direction: column; flex: 1; min-width: 0; overflow: hidden; }

  /* Floating card: bg-base + rounded; the elevated workspace shows in the gaps. */
  .card {
    display: flex;
    flex-shrink: 0;
    min-width: 0;
    min-height: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .card.grow { flex: 1; }
  .card.grow > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
