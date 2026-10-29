<script lang="ts">
  /**
   * The selected entity's components — one editable card each, live while the game runs.
   *
   * Values arrive by push (`+watch`), so a field changing in the game changes here without a poll;
   * the badge says whether that push is active. Editing goes through the typed editors, with the
   * raw-JSON toggle on each card for whatever the registry cannot describe.
   */
  import { ArrowUp, Plus, RefreshCw, Trash2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import BrpValueCard from './BrpValueCard.svelte';
  import { entityTag, shortTypeName } from './brp-value';

  let {
    onAdd,
    onDespawn,
  }: {
    onAdd: () => void;
    onDespawn: (id: number) => void;
  } = $props();

  let filter = $state('');

  const entity = $derived(brp.entity);
  const can = $derived(brp.status.can);

  /** Alphabetical by short name; the game's own order is a hash map's. */
  const entries = $derived.by(() => {
    if (!entity) return [];
    const needle = filter.trim().toLowerCase();
    const paths = new Set([...Object.keys(entity.components), ...Object.keys(entity.errors)]);
    return [...paths]
      .filter((p) => !needle || p.toLowerCase().includes(needle))
      .sort((a, b) => shortTypeName(a).localeCompare(shortTypeName(b)));
  });

  const title = $derived(entity?.name ?? (brp.selected !== null ? 'Entity' : 'Inspector'));
</script>

<PaneHeader {title}>
  {#snippet end()}
    {#if entity}
      <Badge variant="chip" size="sm" label={entityTag(entity.id)} />
      {#if brp.watching}
        <Badge variant="tone" tone="success" size="sm" dot label="Live" />
      {/if}
      {#if entity.parent !== null}
        <IconButton tooltip="Select parent" size={22} onclick={() => entity && entity.parent !== null && void brp.select(entity.parent)}>
          <ArrowUp size={13} />
        </IconButton>
      {/if}
      {#if can.insert}
        <IconButton tooltip="Add component" size={22} onclick={onAdd}>
          <Plus size={13} />
        </IconButton>
      {/if}
      <IconButton tooltip="Refresh" size={22} onclick={() => void brp.reloadEntity()}>
        <RefreshCw size={13} />
      </IconButton>
      {#if can.despawn}
        <IconButton tooltip="Despawn…" variant="danger" size={22} onclick={() => onDespawn(entity!.id)}>
          <Trash2 size={13} />
        </IconButton>
      {/if}
    {/if}
  {/snippet}
</PaneHeader>

{#if brp.selected === null}
  <EmptyState message="Select an entity" description="Pick one in the hierarchy to see and edit its components." />
{:else if brp.entityError && !entity}
  <StateBlock tone="error" label={brp.entityError} />
{:else if entity}
  {#if brp.entityError}
    <StateBlock tone="error" fill={false} label={brp.entityError} />
  {/if}
  <div class="in-filter">
    <Input bind:value={filter} size="sm" clearable placeholder="Filter components…" ariaLabel="Filter components" />
  </div>
  <div class="in-list">
    {#each entries as typePath (typePath)}
      <BrpValueCard
        {typePath}
        value={entity.components[typePath]}
        error={entity.errors[typePath]}
        readonly={!can.mutate}
        onMutate={(path, value) => void brp.mutateComponent(typePath, path, value)}
        onReplace={(value) => brp.insertComponent(typePath, value)}
        onRemove={can.remove ? () => void brp.removeComponent(typePath) : undefined}
        onJump={(id) => void brp.select(id)}
      />
    {:else}
      <EmptyState message={filter ? 'No component matches.' : 'This entity has no components.'} />
    {/each}
  </div>
{/if}

<style>
  .in-filter { padding: 6px 10px; flex: none; }
  .in-list { flex: 1; min-height: 0; overflow-y: auto; padding: 0 10px 10px; display: flex; flex-direction: column; gap: 8px; }
</style>
