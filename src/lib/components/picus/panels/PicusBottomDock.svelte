<script lang="ts">
  /**
   * The bottom dock — **one panel at a time**, chosen by the rail button that
   * opened it.
   *
   * It used to be a tabbed container: one header carrying a four-tab strip, and
   * then each panel's own bar underneath it. That is two rows of chrome for one
   * panel, and it made the rail buttons and the tab strip two ways of saying the
   * same thing that then had to be kept in step. Corvus has had the other
   * arrangement all along — a button opens *its* panel, and the panel owns its
   * title, its count and its actions — and this is now that.
   *
   * Which is why there is almost nothing here: the switch is the whole component,
   * and every panel below draws its own `BottomPanelHeader`.
   */
  import QueryResultPanel from './QueryResultPanel.svelte';
  import ConsistencyPanel from './ConsistencyPanel.svelte';
  import OutputPanel from './OutputPanel.svelte';
  import ChangesPanel from './ChangesPanel.svelte';
  import ActivityPanel from './ActivityPanel.svelte';
  import DependsPanel from './DependsPanel.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
</script>

{#if picusUiStore.bottomTab === 'results'}
  <QueryResultPanel />
{:else if picusUiStore.bottomTab === 'consistency'}
  <ConsistencyPanel />
{:else if picusUiStore.bottomTab === 'output'}
  <OutputPanel />
{:else if picusUiStore.bottomTab === 'activity'}
  <ActivityPanel />
{:else if picusUiStore.bottomTab === 'depends'}
  <DependsPanel />
{:else}
  <ChangesPanel />
{/if}
