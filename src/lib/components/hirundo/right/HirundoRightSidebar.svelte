<script lang="ts">
  /**
   * HirundoRightSidebar — the runner, the histories and the schedules, one per icon of the right rail.
   * They sit on the right, beside the collections you pick from on the left, because they are lists that are
   * read *while* you work in the tree and the editor: a run's results next to the requests they are about.
   */
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { HIRUNDO_RIGHT_SECTIONS } from '../hirundo-palette';
  import HistoryPanel from './HistoryPanel.svelte';
  import RunnerPanel from './RunnerPanel.svelte';
  import SchedulesPanel from './SchedulesPanel.svelte';
  import CookiesPanel from './CookiesPanel.svelte';
  import StreamPanel from './StreamPanel.svelte';

  const section = $derived(HIRUNDO_RIGHT_SECTIONS.find((s) => s.id === hirundoUiStore.rightSection) ?? HIRUNDO_RIGHT_SECTIONS[0]);
</script>

<div class="rs">
  <PaneHeader title={section.label} />
  <div class="rs-body">
    {#if hirundoUiStore.rightSection === 'runner'}<RunnerPanel />
    {:else if hirundoUiStore.rightSection === 'history'}<HistoryPanel />
    {:else if hirundoUiStore.rightSection === 'schedules'}<SchedulesPanel />
    {:else if hirundoUiStore.rightSection === 'cookies'}<CookiesPanel />
    {:else}<StreamPanel />{/if}
  </div>
</div>

<style>
  .rs { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
  .rs-body { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
  .rs-body > :global(*) { flex: 1; min-height: 0; }
</style>
