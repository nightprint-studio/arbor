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
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { Images } from 'lucide-svelte';
  import { takeTytoSnipIntent } from '$lib/ipc/tyto/main-window';
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
  import TytoSelector from './TytoSelector.svelte';
  import TytoCountdown from './TytoCountdown.svelte';
  import { recorderStore, type TargetKind } from '$lib/stores/tyto/recorder.svelte';
  import { tytoUiStore } from '$lib/stores/tyto/ui.svelte';
  import { chord, dispatchKeymap, type KeyRule } from '$lib/utils/keymap';

  let libraryWidth = $state(360);

  // Opened via the OS-global shortcut (quick capture) → drop straight into the Snip
  // selector. A fresh window pulls the intent on mount; an already-open window gets the
  // `tyto://enter-snip` event. Either way we wait for the backend before entering (the
  // selector needs it to freeze/enumerate), with a bounded retry so it never spins.
  //
  // `entryResolved`/`enteringSnip` gate the first paint: until we know whether this is a
  // snip-open — and while entering it — we show a neutral booting screen instead of the
  // full control panel, so a shortcut-open never flashes the full window before the
  // selector covers it (the compact selector is Tyto's "normal" presentation).
  let entryResolved = $state(false);
  let enteringSnip = $state(false);
  onMount(() => {
    let tries = 0;
    function enterSnip() {
      enteringSnip = true;
      if (recorderStore.selecting) return;
      if (recorderStore.backendUp) { void recorderStore.enterSelection('rect'); return; }
      if (tries++ < 50) setTimeout(enterSnip, 100); // ~5s cap while tyto-be attaches
    }
    void takeTytoSnipIntent()
      .then((yes) => { if (yes) { tries = 0; enterSnip(); } })
      .catch(() => {})
      .finally(() => { entryResolved = true; });
    let un: (() => void) | undefined;
    void listen('tyto://enter-snip', () => { void takeTytoSnipIntent().catch(() => {}); tries = 0; enterSnip(); })
      .then((f) => { un = f; });
    return () => un?.();
  });
  // Once the selector is actually up, clear the booting gate so a later "expand to full"
  // (exitSelection) shows the panel, not the booting screen.
  $effect(() => { if (recorderStore.selecting) enteringSnip = false; });

  // After any capture completes the library must surface it — reveal the right rail
  // whenever a fresh capture lands (the store bumps `captureSignal`). The selector has
  // already exited itself on commit, so this only needs to open the library.
  let lastCaptureSignal = 0;
  $effect(() => {
    const sig = recorderStore.captureSignal;
    if (sig !== lastCaptureSignal) {
      lastCaptureSignal = sig;
      if (sig > 0) tytoUiStore.setLibraryOpen(true);
    }
  });

  const railItems = $derived<ActivityRailItem[]>([
    {
      id: 'library',
      // The rail models the two halves separately — a shortcut inlined into the
      // tooltip text renders as prose where every other rail renders a key cap.
      tooltip: 'Captures library',
      shortcut: 'Ctrl+Shift+B',
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

  // Evaluated top to bottom, first hit wins (see `utils/keymap.ts`). Every rule
  // prevents the default, as the hand-written cascade did.
  // Help toggles — available even with a panel open (F1 toggles docs closed), so they are
  // dispatched before the modal gate in `onKeyDown`.
  const helpRules: KeyRule[] = [
    { id: 'toggle_docs',    match: chord({ key: 'F1', shift: false }), run: () => tytoUiStore.toggleDocs() },
    { id: 'open_shortcuts', match: chord({ key: 'F1', shift: true }),  run: () => tytoUiStore.openShortcuts() },
  ];

  const keyRules: KeyRule[] = [
    // Ctrl+Shift+C enters the in-window Snip-style selector (frozen backdrop + toolbar).
    { id: 'snip_selector',   match: chord({ mod: true, shift: true,  key: 'c' }), run: () => void recorderStore.enterSelection('rect') },
    { id: 'open_settings',   match: chord({ mod: true, shift: false, key: ',' }), run: () => tytoUiStore.openSettings() },
    { id: 'toggle_library',  match: chord({ mod: true, shift: true,  key: 'b' }), run: () => tytoUiStore.toggleLibrary() },
    { id: 'reveal_output',   match: chord({ mod: true, shift: true,  key: 'o' }), run: () => void recorderStore.revealOutputFolder() },
    { id: 'cycle_source',    match: chord({ mod: true, shift: true,  key: 's' }), run: () => cycleSource() },
    { id: 'screen_region',   match: chord({ mod: true, shift: true,  key: 'd' }), run: () => void recorderStore.openScreenRegion() },
    // Direct source select — e.code so Shift+digit is layout-robust (Shift+1 = '!' on
    // some layouts), and distinct from the shift-less Ctrl+1/2 mode switches.
    { id: 'source_monitor',  match: chord({ mod: true, shift: true, code: 'Digit1' }), run: () => recorderStore.setTargetKind('monitor') },
    { id: 'source_window',   match: chord({ mod: true, shift: true, code: 'Digit2' }), run: () => recorderStore.setTargetKind('window') },
    { id: 'source_region',   match: chord({ mod: true, shift: true, code: 'Digit3' }), run: () => recorderStore.setTargetKind('region') },
    {
      id: 'toggle_system_audio',
      match: chord({ mod: true, shift: true, key: 'a' }),
      run: () => { if (recorderStore.mode === 'record') recorderStore.toggleSystemAudio(); },
    },
    // Video ↔ frame sequence. The two produce very different files from the same
    // gesture, so the switch deserves to be reachable without leaving the keyboard.
    {
      id: 'toggle_record_output',
      match: chord({ mod: true, shift: true, key: 'f' }),
      run: () => recorderStore.setRecordOutput(recorderStore.recordOutput === 'frames' ? 'video' : 'frames'),
    },
    { id: 'mode_record',     match: chord({ mod: true, shift: false, key: '1' }), run: () => recorderStore.setMode('record') },
    { id: 'mode_screenshot', match: chord({ mod: true, shift: false, key: '2' }), run: () => recorderStore.setMode('screenshot') },
    { id: 'primary_capture', match: chord({ mod: true, key: 'enter' }),           run: () => primaryCapture() },
  ];

  function onKeyDown(e: KeyboardEvent) {
    // The in-window selector / countdown own the keyboard while up (their own Esc / Enter /
    // method keys) — don't let the shell's shortcuts fire behind them.
    if (recorderStore.selecting || recorderStore.countingDown) return;

    if (dispatchKeymap(e, helpRules)) return;

    // Behind a modal, let the dialog own the keyboard (its own Esc, Tab, …).
    if (tytoUiStore.anyModalOpen) return;

    dispatchKeymap(e, keyRules);
  }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if recorderStore.countingDown}
  <TytoCountdown />
{:else if recorderStore.selecting}
  <TytoSelector />
{:else if !entryResolved || enteringSnip}
  <!-- Neutral booting screen: shown until we know this isn't a snip-open (and while the
       selector is being entered), so a shortcut-open never flashes the full panel. -->
  <div class="booting" aria-hidden="true"></div>
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
  /* Booting placeholder — a plain elevated fill so the first frame of a shortcut-open
     shows neither the full panel nor a white flash before the selector takes over. */
  .booting { position: fixed; inset: 0; background: var(--bg-elevated); }
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
