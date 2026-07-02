<script lang="ts">
  /**
   * BennuBottomDock — the bottom tool window (IntelliJ New UI): Problems + Terminal,
   * tabbed. Mirrors Corvus's bottom dock: a shared {@link BottomPanelHeader} owns the
   * chrome (title-less here — the tab strip is the identity), a `Tabs` strip switches
   * sections, and the section-specific actions (terminal "New") sit in the header's
   * actions slot. Its toggles live in the LEFT rail's bottom cluster.
   *
   * The Problems body reuses `BennuProblemsPanel` (header hidden); the Terminal body
   * reuses the generic Corvus `TerminalInstance` via `BennuTerminalView`.
   */
  import { AlertTriangle, TerminalSquare, Plus, Hammer, Square, Trash2 } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import BennuProblemsPanel from './BennuProblemsPanel.svelte';
  import BennuTerminalView from './BennuTerminalView.svelte';
  import BennuBuildPanel from './BennuBuildPanel.svelte';
  import { bennuUiStore, type BottomPanel } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';

  const active = $derived(bennuUiStore.bottomPanel ?? 'problems');

  const tabs: TabItem[] = [
    { id: 'build', label: 'Build', icon: Hammer, iconSize: 13 },
    { id: 'problems', label: 'Problems', icon: AlertTriangle, iconSize: 13 },
    { id: 'terminal', label: 'Terminal', icon: TerminalSquare, iconSize: 13 },
  ];

  let terminalView = $state<{ openTerminal: () => void } | null>(null);
</script>

<div class="dock">
  <BottomPanelHeader onClose={() => bennuUiStore.closeBottom()}>
    {#snippet children()}
      <div class="dock-tabs">
        <Tabs
          items={tabs}
          value={active}
          variant="panel"
          size="sm"
          ariaLabel="Bottom tool windows"
          onSelect={(id) => bennuUiStore.showBottom(id as BottomPanel)}
        />
      </div>
    {/snippet}
    {#snippet actions()}
      {#if active === 'terminal'}
        <button
          class="ps-btn"
          type="button"
          use:tooltip={'New terminal'}
          aria-label="New terminal"
          onclick={() => terminalView?.openTerminal()}
        >
          <Plus size={13} />
        </button>
      {:else if active === 'build'}
        {#if bennuRunStore.running}
          <button
            class="ps-btn"
            type="button"
            use:tooltip={'Stop'}
            aria-label="Stop run"
            onclick={() => void bennuRunStore.stop()}
          >
            <Square size={12} />
          </button>
        {/if}
        <button
          class="ps-btn"
          type="button"
          use:tooltip={'Clear'}
          aria-label="Clear build output"
          disabled={bennuRunStore.active}
          onclick={() => bennuRunStore.clear()}
        >
          <Trash2 size={13} />
        </button>
      {/if}
    {/snippet}
  </BottomPanelHeader>

  <div class="dock-body">
    <!-- Both sections stay mounted so a terminal session survives a tab switch;
         the inactive one is hidden, not destroyed. -->
    <div class="dock-section" class:hidden={active !== 'build'}>
      <BennuBuildPanel />
    </div>
    <div class="dock-section" class:hidden={active !== 'problems'}>
      <BennuProblemsPanel hideHeader />
    </div>
    <div class="dock-section" class:hidden={active !== 'terminal'}>
      <BennuTerminalView bind:this={terminalView} />
    </div>
  </div>
</div>

<style>
  .dock {
    display: flex; flex-direction: column;
    height: 100%; width: 100%; min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }
  .dock-tabs {
    display: flex; align-items: stretch; align-self: stretch;
    margin-left: 6px; min-width: 0;
  }
  .dock-tabs :global(.tabs) { flex: 1; min-width: 0; height: 100%; }

  .dock-body { flex: 1; min-height: 0; position: relative; overflow: hidden; }
  .dock-section {
    position: absolute; inset: 0;
    display: flex; flex-direction: column; min-height: 0;
  }
  .dock-section.hidden { display: none; }
</style>
