<script lang="ts">
  /**
   * The game's resources — a filterable list, and the open one as an editable card.
   *
   * Rendered twice by the panel — `part="list"` in the left pane, `part="detail"` in the right — so
   * both halves of the domain live in one file and the panel knows neither.
   */
  import { Database, RefreshCw, Trash2 } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import BrpValueCard from './BrpValueCard.svelte';

  let { part, onAdd }: { part: 'list' | 'detail'; onAdd?: () => void } = $props();

  let filter = $state('');
  const can = $derived(brp.status.can);

  const nodes = $derived(
    brp.resources
      .filter((r) => !filter.trim() || r.type_path.toLowerCase().includes(filter.trim().toLowerCase()))
      .map((r) => ({ id: r.type_path, res: r, children: [] as never[] })),
  );
</script>

{#if part === 'list'}
  <PaneHeader title="Resources" loading={false}>
    {#snippet end()}
      <span class="rs-count">{nodes.length}</span>
      {#if can.insert_resource && onAdd}
        <IconButton tooltip="Insert a resource…" size={22} onclick={onAdd}>
          <Database size={13} />
        </IconButton>
      {/if}
      <IconButton tooltip="Refresh" size={22} onclick={() => void brp.loadResources()}>
        <RefreshCw size={13} />
      </IconButton>
    {/snippet}
  </PaneHeader>
  <div class="rs-filter">
    <Input bind:value={filter} size="sm" clearable placeholder="Filter resources…" ariaLabel="Filter resources" />
  </div>
  <div class="rs-body">
    {#if !can.resources}
      <EmptyState message="This game does not expose resources." />
    {:else if nodes.length === 0}
      <EmptyState message={filter ? 'No resource matches.' : 'No resources.'} />
    {:else}
      <Tree
        {nodes}
        selectedId={brp.selectedResource}
        onSelect={(n) => void brp.selectResource(n.id)}
        ariaLabel="Resources"
      >
        {#snippet row({ node })}
          <Database size={12} />
          <span class="rs-name">{node.res.short_path}</span>
        {/snippet}
      </Tree>
    {/if}
  </div>
{:else}
  <PaneHeader title={brp.selectedResource ? brp.resources.find((r) => r.type_path === brp.selectedResource)?.short_path : 'Resource'}>
    {#snippet end()}
      {#if brp.selectedResource}
        <IconButton tooltip="Refresh" size={22} onclick={() => void brp.reloadResource()}>
          <RefreshCw size={13} />
        </IconButton>
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
        onJump={(id) => { brp.setTab('entities'); void brp.reveal(id); }}
      />
    </div>
  {/if}
{/if}

<style>
  .rs-filter { padding: 6px 10px; flex: none; }
  .rs-body { flex: 1; min-height: 0; overflow-y: auto; }
  .rs-detail { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 10px; }
  .rs-count { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .rs-name { font-size: var(--font-size-sm); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
