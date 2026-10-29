<script lang="ts">
  /**
   * The build queue: the run in progress and the tickets waiting behind it, with whose they are. A waiting
   * ticket can be withdrawn; the run in progress is stopped from the tool that started it.
   */
  import { X } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import { buildQueueStore as queue } from '$lib/stores/bennu/build-queue.svelte';
  import { ownerName, queueLine } from './build-queue-format';
</script>

{#if queue.rows.length > 0}
  <div class="bq">
    <PaneHeader title="Build queue" count={queue.rows.length} />
    <div class="bq-rows">
      {#each queue.rows as r (r.ticket)}
        <SidebarItem>
          {queueLine(r)}
          {#snippet subtitle()}{ownerName(r.owner)} · {r.label}{/snippet}
          {#snippet badges()}
            <Badge variant="tone" size="sm" dot tone={r.state === 'running' ? 'success' : 'warning'} label={r.state === 'riding' ? 'shares a run' : r.state} />
          {/snippet}
          {#snippet actions()}
            {#if r.state !== 'running'}
              <IconButton tooltip="Cancel this ticket" size={20} onclick={() => void queue.cancel(r.ticket)}><X size={11} /></IconButton>
            {/if}
          {/snippet}
        </SidebarItem>
      {/each}
    </div>
  </div>
{/if}

<style>
  .bq { flex: none; max-height: 150px; display: flex; flex-direction: column; border-bottom: 1px solid var(--border-subtle, var(--border)); }
  .bq-rows { overflow-y: auto; min-height: 0; }
</style>
