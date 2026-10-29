<script lang="ts">
  /**
   * The Live tool window — the project's running services: start a stack from a recipe, read what each
   * process prints, replay a scenario, follow one request through every log.
   *
   * Everything on screen is composed from shared widgets. Mounted only while shown (the store's
   * subscription and refreshes stop when it closes); the processes themselves keep running, and the
   * assistant can still see and drive them.
   */
  import { onMount } from 'svelte';
  import { Activity, RefreshCw } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { bennuLiveStore as live, type LiveTab } from '$lib/stores/bennu/live.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { buildQueueStore as buildQueue } from '$lib/stores/bennu/build-queue.svelte';
  import LiveBuildQueue from './LiveBuildQueue.svelte';
  import LiveProcesses from './LiveProcesses.svelte';
  import LiveRecipes from './LiveRecipes.svelte';
  import LiveScenarios from './LiveScenarios.svelte';
  import LiveTimeline from './LiveTimeline.svelte';

  const TABS = $derived([
    { id: 'processes', label: 'Processes' },
    { id: 'recipes', label: 'Recipes' },
    { id: 'scenarios', label: 'Scenarios' },
    { id: 'timeline', label: 'Timeline' },
  ]);

  onMount(() => {
    void live.mount();
    void buildQueue.mount();
    return () => {
      live.unmount();
      buildQueue.unmount();
    };
  });

  const banner = $derived(live.error || live.state?.notice || '');
</script>

<div class="lp">
  <BottomPanelHeader title="Live" onClose={() => bennuUiStore.closeBottom()}>
    {#snippet icon()}<Activity size={13} />{/snippet}
    <Tabs items={TABS} value={live.tab} variant="pill" size="sm" onSelect={(id) => void live.setTab(id as LiveTab)} />
    {#if live.running > 0}
      <Badge variant="tone" size="sm" dot tone="success" label={`${live.running} running`} />
    {/if}
    {#snippet actions()}
      <IconButton tooltip="Refresh" size={22} onclick={() => void live.refresh()}><RefreshCw size={13} /></IconButton>
    {/snippet}
  </BottomPanelHeader>
  {#if banner}
    <Alert variant={live.error ? 'error' : 'warning'} compact text={banner} />
  {/if}
  <LiveBuildQueue />
  <div class="lp-body">
    {#if live.tab === 'processes'}
      <LiveProcesses />
    {:else if live.tab === 'recipes'}
      <LiveRecipes />
    {:else if live.tab === 'scenarios'}
      <LiveScenarios />
    {:else}
      <LiveTimeline />
    {/if}
  </div>
</div>

<style>
  .lp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .lp-body { flex: 1; min-height: 0; }
</style>
