<script lang="ts">
  /**
   * The selected resource of the game as an editable section. (The resources themselves are listed in the
   * sidebar tree; this is only the detail.)
   */
  import { Database, RefreshCw, Trash2 } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import BrpCopyMenu from './BrpCopyMenu.svelte';
  import BrpIcon from './icons/BrpIcon.svelte';
  import BrpValueCard from './BrpValueCard.svelte';
  import { shortTypeName } from './brp-value';

  let { onAdd }: { onAdd?: () => void } = $props();

  const can = $derived(brp.status.can);
</script>

<PaneHeader>
  {#snippet children()}
    <BrpIcon name="resource" size={14} />
    {brp.selectedResource ? shortTypeName(brp.selectedResource) : 'Resource'}
  {/snippet}
  {#snippet end()}
    {#if can.insert_resource && onAdd}
      <IconButton tooltip="Insert a resource…" size={22} onclick={onAdd}><Database size={13} /></IconButton>
    {/if}
    {#if brp.selectedResource}
      <BrpCopyMenu what="resource" value={() => brp.resourceValue} />
      <IconButton tooltip="Refresh" size={22} onclick={() => void brp.reloadResource()}><RefreshCw size={13} /></IconButton>
      {#if can.remove_resource}
        <IconButton
          tooltip="Remove resource"
          variant="danger"
          size={22}
          onclick={() => brp.selectedResource && void brp.removeResource(brp.selectedResource)}
        >
          <Trash2 size={13} />
        </IconButton>
      {/if}
    {/if}
  {/snippet}
</PaneHeader>

{#if brp.selectedResource === null}
  <EmptyState message="Select a resource" description="Resources are the game's singleton data: time, scores, state machines." />
{:else if brp.resourceError}
  <StateBlock tone="error" label={brp.resourceError} />
{:else if brp.resourceValue !== undefined}
  {@const path = brp.selectedResource}
  <div class="rs-detail">
    <BrpValueCard
      typePath={path}
      value={brp.resourceValue}
      readonly={!can.mutate_resource}
      onMutate={(p, v) => void brp.mutateResource(path, p, v)}
      onReplace={(v) => brp.replaceResource(path, v)}
      onJump={(id) => { void brp.reveal(id); }}
    />
  </div>
{/if}

<style>
  .rs-detail { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 8px 8px; }
</style>
