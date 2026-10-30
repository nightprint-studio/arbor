<script lang="ts">
  /**
   * What the profiler is doing right now, and the button to end it. Starting a recording is not
   * done here: the title bar's profiler button records the selected run configuration, and
   * **Profile…** (the ⋮ menu, the palette, the gauge in the panel header) opens the dialog for a command or a
   * running JVM. This panel reads recordings.
   */
  import { Circle, Square } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
</script>

{#if store.phase !== 'idle' || store.error}
<div class="rec">
  {#if store.phase === 'recording'}
    <span class="live"><Circle size={9} fill="currentColor" /> Recording{store.live ? ` — ${store.live.lang}` : ''}</span>
    <Button size="sm" variant="danger" onclick={() => void store.stop()}>
      {#snippet iconStart()}<Square size={11} />{/snippet}
      Stop and read
    </Button>
  {:else if store.phase === 'parsing'}
    <Spinner size={13} label={store.progress ? `Reading the recording… ${store.progress.toLocaleString()} events` : 'Reading the recording…'} />
  {:else if store.phase === 'preparing'}
    <Spinner size={13} label="Preparing…" />
  {/if}
  {#if store.error}
    <Alert variant="error" compact>{store.error}</Alert>
  {/if}
</div>
{/if}

<style>
  .rec { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; padding: 6px 8px; }
  .live { display: inline-flex; align-items: center; gap: 5px; color: var(--danger); font-size: var(--font-size-xs); }
</style>
