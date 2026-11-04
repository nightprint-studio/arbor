<script lang="ts">
  /**
   * The bottom panel — one thing, the answer to the open request, in the same card and header every product's
   * bottom panel uses (resizable by its top edge, slides in and out). No tab strip: the runner, the histories
   * and the schedules are not "views of the answer", they have their own icons on the right rail.
   */
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import ResponseViewer from '../response/ResponseViewer.svelte';

  let { height = 300, onResize }: { height?: number; onResize?: (px: number) => void } = $props();
</script>

<PanelCard orientation="bottom" initialSize={height} minSize={140} maxSize={640} {onResize}>
  <div class="bp">
    <BottomPanelHeader title="Response" onClose={() => hirundoUiStore.closeBottom()} />
    <div class="bp-body">
      {#if hirundoStore.active}
        {#key hirundoStore.active.id}<ResponseViewer tab={hirundoStore.active} />{/key}
      {:else}
        <EmptyState message="No request open" description="The answer of the open request shows here." compact />
      {/if}
    </div>
  </div>
</PanelCard>

<style>
  .bp { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
  .bp-body { flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column; }
  .bp-body > :global(*) { flex: 1; min-height: 0; }
</style>
