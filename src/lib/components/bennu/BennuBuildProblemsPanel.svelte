<script lang="ts">
  /**
   * Build + Problems — the one bottom panel that carries two views, because they are two
   * readings of the same run: what the build said, and what the checks found.
   *
   * Every other bottom tool window is its own panel opened by its own rail button (see
   * {@link BennuBottomDock}). These two share a panel and a header, and the rail button that
   * opened it decides which of the two is showing — the header's two-tab strip is the same
   * switch, reachable without going back to the rail.
   *
   * The panel owns its chrome: the title strip, the section's actions (Stop / Clear while a
   * build is what you are looking at) and the close button.
   */
  import { Hammer, Trash2 } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import BennuBuildPanel from './BennuBuildPanel.svelte';
  import BennuProblemsPanel from './BennuProblemsPanel.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';

  /** Which of the two the rail opened. Anything else means Build. */
  const section = $derived(bennuUiStore.bottomPanel === 'problems' ? 'problems' : 'build');

  // The badge is the whole-project count: the number that answers "is there anything to look at"
  // without switching to the tab to find out.
  const problemCount = $derived(
    bennuDiagnosticsStore.projectProblemCount + bennuDiagnosticsStore.mojibakeProblemCount,
  );

  const tabs = $derived<TabItem[]>([
    { id: 'build', label: 'Build', icon: Hammer, iconSize: 13 },
    { id: 'problems', label: 'Problems', badge: problemCount || undefined },
  ]);
</script>

<div class="bpp">
  <BottomPanelHeader onClose={() => bennuUiStore.closeBottom()}>
    {#snippet children()}
      <div class="bpp-tabs">
        <Tabs
          items={tabs}
          value={section}
          variant="panel"
          size="sm"
          ariaLabel="Build and Problems"
          onSelect={(id) => bennuUiStore.showBottom(id as 'build' | 'problems')}
        />
      </div>
    {/snippet}
    {#snippet actions()}
      {#if section === 'build'}
        <!-- No Stop here any more: what was being stopped is the launched PROGRAM, and it
             now has its own console with its own Stop. A build is not cancellable. -->
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

  <!-- Build stays mounted so a streaming log keeps its scroll while you read the problems. -->
  <div class="bpp-body">
    <div class="bpp-section" class:hidden={section !== 'build'}>
      <BennuBuildPanel />
    </div>
    <div class="bpp-section" class:hidden={section !== 'problems'}>
      <BennuProblemsPanel hideHeader />
    </div>
  </div>
</div>

<style>
  .bpp {
    display: flex; flex-direction: column;
    height: 100%; width: 100%; min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }
  .bpp-tabs {
    display: flex; align-items: stretch; align-self: stretch;
    margin-left: 6px; min-width: 0;
  }
  .bpp-tabs :global(.tabs) { flex: 1; min-width: 0; height: 100%; }

  .bpp-body { flex: 1; min-height: 0; position: relative; overflow: hidden; }
  .bpp-section {
    position: absolute; inset: 0;
    display: flex; flex-direction: column; min-height: 0;
  }
  .bpp-section.hidden { display: none; }
</style>
