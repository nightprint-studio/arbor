<script lang="ts">
  /**
   * The right-hand end of a checked row: its verdict, and the way to ask again. A verdict that could
   * not be asked for again is how a stale "valid" outlives the edit that broke it, so a row that has
   * been checked keeps a re-check button beside its badge.
   */
  import { RotateCw } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import type { NativeOutcome } from '$lib/stores/bennu/picus.svelte';

  let { outcome, oncheck }: { outcome: NativeOutcome | undefined; oncheck: () => void } = $props();
</script>

<span class="cell">
  {#if !outcome}
    <Button size="xs" variant="ghost" onclick={oncheck}>Check</Button>
  {:else if outcome.state === 'checking'}
    <Spinner size={12} />
  {:else}
    {#if outcome.state === 'ok'}
      <Badge variant="tone" tone="success" label="valid" />
    {:else if outcome.state === 'rejected'}
      <Badge variant="tone" tone="error" label="rejected" />
    {:else}
      <Badge variant="tone" tone="warning" label="not checked" />
    {/if}
    <IconButton tooltip="Check again" size={20} onclick={oncheck}><RotateCw size={11} /></IconButton>
  {/if}
</span>

<style>
  /* Never shrinks: the name beside it gives way, not the verdict. */
  .cell { flex: none; display: inline-flex; align-items: center; gap: 2px; }
</style>
