<script lang="ts">
  /**
   * GarrulusShell — the notes window.
   *
   * Arbor's standard layout, unchanged: title bar · activity rail · sidebar ·
   * centre column · status bar, on the shared `WorkspaceShell` / `ActivityBar` /
   * `PanelCard` chrome. Someone coming from Corvus or Picus should not notice
   * they changed application. The geometry follows `docs/mockups/garrulus-ui.html`,
   * which is the approved reference for this window.
   *
   * This file owns three things and delegates the rest: the layout, the keyboard,
   * and the lifetime of the two stores the window reads from. The panels are
   * their own components (`GarrulusSidebar`, `GarrulusStartPane`) and every
   * overlay lives in `GarrulusOverlays`, so the shell stays readable as a layout
   * while the catalogue of verbs grows.
   *
   * **The vault, the sync destination and the command palette are live.** The
   * note tree, the editor, the search view and the bottom dock still arrive with
   * the domain that can serve them, and the surfaces that would show them state
   * what they will hold rather than faking content.
   */
  import { onMount } from 'svelte';
  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import FeedbackHost from '$lib/feedback/FeedbackHost.svelte';
  import FeedbackStatusButtons from '$lib/feedback/FeedbackStatusButtons.svelte';
  import GarrulusTitleBar from './shell/GarrulusTitleBar.svelte';
  import GarrulusStatusBar from './shell/GarrulusStatusBar.svelte';
  import GarrulusSidebar from './GarrulusSidebar.svelte';
  import GarrulusStartPane from './GarrulusStartPane.svelte';
  import GarrulusOverlays from './GarrulusOverlays.svelte';
  import GarrulusConflictsDock from './panels/GarrulusConflictsDock.svelte';
  import GarrulusSearchView from './search/GarrulusSearchView.svelte';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';
  import { GARRULUS_SECTIONS, garrulusPaletteIcon } from './garrulus-palette';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { garrulusUiStore } from '$lib/stores/garrulus/ui.svelte';
  import { garrulusSyncStore } from '$lib/stores/garrulus/sync.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';

  /** Sidebar width. Session-only UI state, like every other product's. */
  let sidebarWidth = $state(260);
  /** Bottom-dock height. Session-only too — the dock's *visibility* is in the ui
   *  store because the palette and the sync control both open it. */
  let dockHeight = $state(280);

  // Both stores subscribe to `arbor://garrulus-be-up` themselves (the backend
  // spawns off-thread and races this window's first reads). The shell only owns
  // their lifetime. Neither opens a vault: they read, and the start pane offers.
  onMount(() => {
    void garrulusSyncStore.init();
    void garrulusVaultStore.init();
    return () => {
      garrulusSyncStore.dispose();
      garrulusVaultStore.dispose();
    };
  });

  const section = $derived(
    GARRULUS_SECTIONS.find((s) => s.id === garrulusUiStore.sidebarSection) ?? GARRULUS_SECTIONS[0],
  );

  const railTop = $derived<ActivityRailItem[]>(
    GARRULUS_SECTIONS.map((s) => ({
      id: s.id,
      icon: garrulusPaletteIcon(s.icon),
      tooltip: s.label,
      shortcut: s.shortcut,
      active: garrulusUiStore.sidebarOpen && garrulusUiStore.sidebarSection === s.id,
      onclick: () => garrulusUiStore.showSection(s.id),
    })),
  );

  /**
   * Keyboard.
   *
   * Order is load-bearing, and it is the order the rest of the suite uses:
   *
   *  1. **the focus gate** — in the tabbed container every mounted surface hears
   *     `svelte:window`, so a shell that skips this fires its shortcuts from a
   *     hidden tab (`Ctrl+Shift+S` here and Corvus's binding would both run);
   *  2. **help and the palette** — F1 has to close the panel it opened from
   *     wherever focus went, and the palette is itself part of `anyModalOpen`,
   *     so a `Ctrl+K` tested after that guard could open it and never close it;
   *  3. **the modal guard** — behind any other dialog, let it own the keyboard;
   *  4. **the rest**, which all carry a modifier.
   *
   * `e.code` for the digits so they survive non-US layouts, and never
   * `Ctrl+Alt+<letter>` (AltGr eats it on IT/DE/FR/ES).
   * `garrulus-shortcuts.ts` is the canonical list; keep the two in step.
   */
  function onKeyDown(e: KeyboardEvent) {
    if (!surfaceStore.hasFocus('garrulus')) return;

    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    // Help reads beside the work rather than in front of it, so it answers even
    // with a dialog up — and F1 again puts it away.
    if (e.key === 'F1') {
      if (e.shiftKey) garrulusUiStore.openDocs('shortcuts');
      else garrulusUiStore.toggleDocs();
      e.preventDefault();
      return;
    }

    if (garrulusUiStore.anyModalOpen) {
      // The palette's own toggle, before the guard swallows it.
      if (mod && key === 'k' && garrulusUiStore.paletteOpen) {
        garrulusUiStore.closePalette();
        e.preventDefault();
      }
      return;
    }

    if (!mod) return;

    if (key === 'k' && !e.shiftKey) {
      garrulusUiStore.togglePalette();
      e.preventDefault();
      return;
    }

    if (e.shiftKey && key === 'o') {
      // The door to the product. Not `Ctrl+O`: that is the note quick switcher,
      // and the two questions ("which vault" / "which note") are not the same.
      garrulusUiStore.openVaultPicker('open');
      e.preventDefault();
      return;
    }

    if (e.shiftKey && key === 's') {
      // Always leads somewhere: with no destination configured there is nothing
      // to sync, so the shortcut opens the form that fixes that.
      if (garrulusSyncStore.tag === 'no-remote') garrulusUiStore.openRemoteConfig();
      else void garrulusSyncStore.syncNow();
      e.preventDefault();
      return;
    }

    if (!e.shiftKey && key === 'b') {
      garrulusUiStore.toggleSidebar();
      e.preventDefault();
      return;
    }

    if (!e.shiftKey) {
      const i = GARRULUS_SECTIONS.findIndex((_, idx) => e.code === `Digit${idx + 1}`);
      if (i !== -1) {
        garrulusUiStore.selectSection(GARRULUS_SECTIONS[i].id);
        e.preventDefault();
      }
    }
  }
</script>

<svelte:window
  onkeydown={onKeyDown}
  onfocus={() => surfaceStore.hasFocus('garrulus') && garrulusSyncStore.setFocused(true)}
  onblur={() => surfaceStore.hasFocus('garrulus') && garrulusSyncStore.setFocused(false)}
/>

<div class="shell">
  <GarrulusTitleBar
    vaultName={garrulusVaultStore.name}
    onConflicts={() => garrulusUiStore.showDock()}
  />

  <div class="content-area">
    <WorkspaceShell>
      {#snippet leftRail()}
        <ActivityBar side="left" ariaLabel="Garrulus sections" topItems={railTop} />
      {/snippet}

      {#snippet panels()}
        {#if garrulusUiStore.sidebarOpen}
          <PanelCard
            orientation="left"
            initialSize={sidebarWidth}
            minSize={200}
            maxSize={460}
            onResize={(px) => (sidebarWidth = px)}
          >
            <GarrulusSidebar {section} />
          </PanelCard>
        {/if}

        <div class="main-col">
          <div class="card grow">
            <!-- Search is a CENTRE view, not a sidebar panel: it carries a preview
                 pane beside its results, which is what the mockup shows and what a
                 260px rail could not hold. Selecting its section swaps the body. -->
            {#if garrulusUiStore.sidebarSection === 'search' && garrulusVaultStore.isOpen}
              <GarrulusSearchView onOpenNote={(p) => void garrulusNotesStore.openNote(p)} />
            {:else}
              <GarrulusStartPane />
            {/if}
          </div>
          {#if garrulusUiStore.dockOpen}
            <!-- `onOpenNote` is not optional in practice: an editor IS mounted in
                 this window, so without it every "open this note" row in Tasks,
                 Problems and Conflicts falls back to its no-editor branch and
                 offers a verb that goes nowhere. -->
            <GarrulusConflictsDock
              height={dockHeight}
              onResize={(px) => (dockHeight = px)}
              onOpenNote={(p) => void garrulusNotesStore.openNote(p)}
              onClose={() => garrulusUiStore.closeDock()}
            />
          {/if}
        </div>
      {/snippet}
    </WorkspaceShell>
  </div>

  <GarrulusStatusBar
    vaultName={garrulusVaultStore.name}
    noteCount={garrulusVaultStore.noteCount}
  >
    {#snippet footerExtra()}
      <FeedbackStatusButtons />
    {/snippet}
  </GarrulusStatusBar>
</div>

<!-- Dialogs and the command palette, owned as a group rather than by whichever
     control opens each: most of them have three or four ways in. -->
<GarrulusOverlays />

<Tooltip />

<!-- Toasts / notifications / progress addressed to this window. -->
<FeedbackHost id="garrulus" />

<style>
  .shell {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
    overflow: hidden;
  }

  /* A few px of bg-elevated under the titlebar so the floating panel cards read
     as detached from the chrome (IntelliJ New UI). WorkspaceShell has no top
     padding by design; the window adds it. */
  .content-area {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding-top: 6px;
    background: var(--bg-elevated);
  }

  .main-col {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    gap: 4px;
  }

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
