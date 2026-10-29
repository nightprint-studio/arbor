<script lang="ts">
  /**
   * The game's type registry, browsable: every component and resource it registered.
   *
   * What the typed editors and the "add component" picker are built from, shown as a list so a
   * type's exact path — the thing the Remote Protocol wants and nobody remembers — can be found and
   * copied. Rendered twice by the panel (`part="list"`, `part="detail"`), like the resources pair.
   */
  import { Layers, Plus } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import type { BrpTypeInfo, BrpTypeKind } from '$lib/ipc/bennu/brp';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';

  let {
    part,
    onAddToEntity,
  }: {
    part: 'list' | 'detail';
    /** Open the add dialog for the open type's kind. */
    onAddToEntity?: (type: BrpTypeInfo, kind: BrpTypeKind) => void;
  } = $props();

  let query = $state('');
  $effect(() => { query = brp.typeQuery; });

  // The open type lives in the store: the list and the detail are two instances of this component.
  const open = $derived(brp.types.items.find((t) => t.type_path === brp.selectedType) ?? null);
  const canUse = $derived(
    brp.typeKind === 'component'
      ? brp.selected !== null && brp.status.can.insert
      : brp.status.can.insert_resource,
  );

  const nodes = $derived(brp.types.items.map((t) => ({ id: t.type_path, info: t, children: [] as never[] })));
  const tabs = [
    { id: 'component', label: 'Components' },
    { id: 'resource', label: 'Resources' },
  ];
</script>

{#if part === 'list'}
  <PaneHeader title="Registered types">
    {#snippet end()}
      <span class="ty-count">{brp.types.items.length}{brp.types.total > brp.types.items.length ? ` of ${brp.types.total}` : ''}</span>
    {/snippet}
  </PaneHeader>
  <div class="ty-bar">
    <Tabs items={tabs} value={brp.typeKind} size="sm" variant="pill" fill onSelect={(id) => void brp.searchTypes(undefined, id as BrpTypeKind)} />
    <SearchBar
      bind:query
      showRegex={false}
      showCounter={false}
      placeholder="Search by name or path…"
      ariaLabel="Search registered types"
      oninput={(v) => void brp.searchTypes(v)}
      onClear={() => void brp.searchTypes('')}
    />
  </div>
  <div class="ty-body">
    {#if nodes.length === 0}
      <EmptyState
        message="No registered type matches."
        description={brp.status.type_count === 0 ? 'The game did not expose its type registry.' : undefined}
      />
    {:else}
      <Tree {nodes} selectedId={brp.selectedType} onSelect={(n) => brp.selectType(n.id)} ariaLabel="Registered types">
        {#snippet row({ node })}
          <Layers size={12} />
          <span class="ty-name">{node.info.short_path}</span>
          {#if node.info.crate_name}<Badge variant="chip" size="sm" label={node.info.crate_name} />{/if}
        {/snippet}
      </Tree>
    {/if}
  </div>
{:else}
  <PaneHeader title={open?.short_path ?? 'Type'} />
  {#if !open}
    <EmptyState message="Select a type" description="Its exact path, kind and what it can be used as are shown here." />
  {:else}
    <div class="ty-detail">
      <div class="ty-path">
        <code>{open.type_path}</code>
        <CopyButton value={open.type_path} />
      </div>
      <div class="ty-badges">
        {#if open.kind}<Badge variant="chip" size="sm" label={open.kind} />{/if}
        {#each open.reflect_types as r (r)}
          <Badge variant="tone" tone={r === 'Component' || r === 'Resource' ? 'accent' : 'neutral'} size="sm" label={r} />
        {/each}
      </div>
      {#if onAddToEntity && canUse}
        <Button variant="primary" size="sm" onclick={() => onAddToEntity(open, brp.typeKind)}>
          {#snippet iconStart()}<Plus size={12} />{/snippet}
          {brp.typeKind === 'component' ? 'Add to selected entity' : 'Insert resource'}
        </Button>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .ty-bar { padding: 6px 8px; flex: none; display: flex; flex-direction: column; gap: 6px; }
  .ty-body { flex: 1; min-height: 0; overflow-y: auto; }
  .ty-detail { padding: 10px 12px; display: flex; flex-direction: column; gap: 10px; align-items: flex-start; }
  .ty-path { display: flex; align-items: center; gap: 6px; }
  .ty-path code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); word-break: break-all; }
  .ty-badges { display: flex; flex-wrap: wrap; gap: 4px; }
  .ty-count { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .ty-name { font-size: var(--font-size-sm); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
