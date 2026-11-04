<script lang="ts">
  /**
   * HirundoShell — the HTTP client window: title bar · activity rail · sidebar · request area · bottom panel ·
   * status bar, on the shared `WorkspaceShell` / `ActivityBar` / `PanelCard` chrome, the same layout as every
   * other product. The sidebar holds what you pick from (collections, detected endpoints, environments), the
   * centre what you write, the bottom panel what came back and what ran. This file owns the layout and the
   * keyboard; the panels, the verbs and the overlays are their own files.
   */
  import { paletteIcon } from '$lib/utils/plugin-icons';
  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import FeedbackHost from '$lib/feedback/FeedbackHost.svelte';
  import FeedbackStatusButtons from '$lib/feedback/FeedbackStatusButtons.svelte';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { hirundoContextMenuStore } from '$lib/stores/hirundo/contextmenu.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { hirundoSchedules } from '$lib/stores/hirundo/schedules.svelte';
  import { hirundoSse } from '$lib/stores/hirundo/sse.svelte';
  import { onMount } from 'svelte';
  import HirundoTitleBar from './shell/HirundoTitleBar.svelte';
  import HirundoStatusBar from './shell/HirundoStatusBar.svelte';
  import HirundoSidebar from './sidebar/HirundoSidebar.svelte';
  import HirundoBottomPanel from './bottom/HirundoBottomPanel.svelte';
  import RequestPane from './editor/RequestPane.svelte';
  import HirundoRightSidebar from './right/HirundoRightSidebar.svelte';
  import HirundoOverlays from './HirundoOverlays.svelte';
  import { HIRUNDO_RIGHT_SECTIONS, HIRUNDO_SECTIONS } from './hirundo-palette';
  import { hirundoCommands as c } from './hirundo-commands';

  /** Panel sizes. Session-only UI state, like every other product's. */
  let sidebarWidth = $state(290);
  let bottomHeight = $state(300);
  let rightWidth = $state(340);

  // Schedules are evaluated by a tick while this window lives.
  $effect(() => {
    void hirundoStore.project;
    void hirundoSchedules.load();
  });

  onMount(() => {
    hirundoSchedules.start();
    return () => hirundoSchedules.stop();
  });

  const railTop = $derived<ActivityRailItem[]>(
    HIRUNDO_SECTIONS.map((s) => ({
      id: s.id,
      icon: paletteIcon(s.icon),
      tooltip: s.label,
      shortcut: s.shortcut,
      active: hirundoUiStore.sidebarOpen && hirundoUiStore.section === s.id,
      onclick: () => hirundoUiStore.showSection(s.id),
    })),
  );

  const railRight = $derived<ActivityRailItem[]>(
    HIRUNDO_RIGHT_SECTIONS.map((s) => ({
      id: s.id,
      icon: paletteIcon(s.icon),
      tooltip: s.label,
      shortcut: s.shortcut,
      active: hirundoUiStore.rightOpen && hirundoUiStore.rightSection === s.id,
      onclick: () => hirundoUiStore.toggleRight(s.id),
    })),
  );

  const railBottom = $derived<ActivityRailItem[]>([
    {
      id: 'bottom-panel',
      icon: paletteIcon('PanelBottom'),
      tooltip: 'Response panel',
      shortcut: 'Ctrl+J',
      active: hirundoUiStore.bottomOpen,
      onclick: () => hirundoUiStore.toggleBottom(),
    },
  ]);

  /**
   * Keyboard. Order is load-bearing: the focus gate first (in the tabbed container every mounted surface
   * hears `window`), then help and the palette toggle (which must work from behind any dialog), then the
   * modal guard, then the rest. `e.code` for digits so they survive non-US layouts; never `Ctrl+Alt+letter`.
   * `hirundo-shortcuts.ts` is the canonical list; keep the two in step.
   */
  function onKeyDown(e: KeyboardEvent) {
    if (!surfaceStore.hasFocus('hirundo')) return;
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    if (e.key === 'F1') {
      if (e.shiftKey) hirundoUiStore.openDocs('shortcuts'); else hirundoUiStore.toggleDocs();
      e.preventDefault();
      return;
    }

    if (hirundoUiStore.anyModalOpen || hirundoStore.pending) {
      if (mod && key === 'k' && hirundoUiStore.paletteOpen) { hirundoUiStore.closePalette(); e.preventDefault(); }
      return;
    }

    if (e.key === 'Escape' && hirundoStore.active?.sending) { c.cancel(); e.preventDefault(); return; }
    if (e.key === 'Escape' && hirundoSse.running) { void hirundoSse.stop(); e.preventDefault(); return; }
    if (!mod) return;

    const open = !!hirundoStore.project;
    const has = !!hirundoStore.active;
    let handled = true;

    if (key === 'k' && !e.shiftKey) hirundoUiStore.togglePalette();
    else if (e.shiftKey && key === 'o') c.openProject();
    else if (e.shiftKey && key === 'n' && open) c.newCollection();
    else if (e.shiftKey && key === 's' && has) c.saveAs();
    else if (e.key === 'Enter' && has) c.send();
    else if (e.shiftKey && key === 'u' && open) c.curl();
    else if (e.shiftKey && key === 'y' && open) c.importOpenApi();
    else if (e.shiftKey && key === 'l' && has) c.startStream();
    else if (e.shiftKey && key === 'f' && has) c.formatBody();
    else if (e.shiftKey && key === 'b' && has) c.toggleBulk();
    else if (e.shiftKey && key === 'v' && has) c.variables();
    else if (!e.shiftKey && key === 's' && has) c.save();
    else if (!e.shiftKey && key === 'd' && has) c.duplicate();
    else if (!e.shiftKey && key === 'n' && open) c.newRequest();
    else if (!e.shiftKey && key === 'p' && open) c.findRequest();
    else if (!e.shiftKey && key === 'e' && open) c.environments();
    else if (!e.shiftKey && key === 'w' && has) c.closeTab();
    else if (!e.shiftKey && key === 'b') hirundoUiStore.toggleSidebar();
    else if (!e.shiftKey && key === 'j') hirundoUiStore.toggleBottom();
    else if (e.key === 'PageDown') hirundoStore.step(1);
    else if (e.key === 'PageUp') hirundoStore.step(-1);
    else if (!e.shiftKey && open) {
      // Ctrl+1..3 the left sidebar, Ctrl+4..6 the right one.
      const digit = [1, 2, 3, 4, 5, 6, 7, 8].find((n) => e.code === `Digit${n}`);
      const left = HIRUNDO_SECTIONS.length;
      if (digit && digit <= left) hirundoUiStore.selectSection(HIRUNDO_SECTIONS[digit - 1].id);
      else if (digit && digit - left <= HIRUNDO_RIGHT_SECTIONS.length) hirundoUiStore.showRight(HIRUNDO_RIGHT_SECTIONS[digit - left - 1].id);
      else handled = false;
    } else handled = false;
    if (handled) e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="shell">
  <HirundoTitleBar />

  <div class="content-area">
    <WorkspaceShell>
      {#snippet leftRail()}
        <ActivityBar side="left" ariaLabel="Hirundo sections" topItems={railTop} bottomItems={railBottom} />
      {/snippet}

      {#snippet rightRail()}
        <ActivityBar side="right" ariaLabel="Hirundo activity" topItems={railRight} />
      {/snippet}

      {#snippet panels()}
        {#if hirundoUiStore.sidebarOpen}
          <PanelCard orientation="left" initialSize={sidebarWidth} minSize={220} maxSize={560} onResize={(px) => (sidebarWidth = px)}>
            <HirundoSidebar />
          </PanelCard>
        {/if}
        <div class="main-col">
          <div class="card grow"><RequestPane /></div>
          {#if hirundoUiStore.bottomOpen}
            <HirundoBottomPanel height={bottomHeight} onResize={(px) => (bottomHeight = px)} />
          {/if}
        </div>
        {#if hirundoUiStore.rightOpen}
          <PanelCard orientation="right" initialSize={rightWidth} minSize={260} maxSize={620} onResize={(px) => (rightWidth = px)}>
            <HirundoRightSidebar />
          </PanelCard>
        {/if}
      {/snippet}
    </WorkspaceShell>
  </div>

  <HirundoStatusBar>
    {#snippet footerExtra()}<FeedbackStatusButtons />{/snippet}
  </HirundoStatusBar>
</div>

<HirundoOverlays />
{#if hirundoContextMenuStore.open}
  <ContextMenu
    items={hirundoContextMenuStore.items}
    actions={hirundoContextMenuStore.actions}
    x={hirundoContextMenuStore.x}
    y={hirundoContextMenuStore.y}
    above={hirundoContextMenuStore.above}
    onSelect={(id) => hirundoContextMenuStore.select(id)}
    onClose={() => hirundoContextMenuStore.close()}
  />
{/if}
<Tooltip />
<FeedbackHost id="hirundo" />

<style>
  .shell { position: fixed; inset: 0; display: flex; flex-direction: column; background: var(--bg-base); overflow: hidden; }
  .content-area { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; padding-top: 6px; background: var(--bg-elevated); }
  .main-col { display: flex; flex-direction: column; flex: 1; min-width: 0; overflow: hidden; gap: 4px; }
  .card { display: flex; flex-shrink: 0; min-width: 0; min-height: 0; background: var(--bg-base); border-radius: var(--radius-lg); overflow: hidden; }
  .card.grow { flex: 1; }
  .card.grow > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
