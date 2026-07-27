<script lang="ts">
  /**
   * Inventory panel — every object the scripts define or touch, and whether both
   * branches tell the same story about it.
   *
   * An object with a zero in any branch's column is a gap: something exists in
   * Oracle and not in PostgreSQL (or in initialisation and not in updates). That
   * is the `CONS001`/`CONS002` family, surfaced here as a marker before the
   * analysis has even been asked for.
   */
  import { Layers, Table2, Package, RefreshCw, TriangleAlert } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import type { InventoryObject } from '$lib/types/picus';

  let query = $state('');
  const needle = $derived(query.trim().toLowerCase());

  const visible = $derived(
    picusProjectStore.inventory.filter((o) => !needle || o.name.toLowerCase().includes(needle)),
  );

  /** Which branch/folder slots exist, in tree order — the coverage columns. */
  const slots = $derived(
    picusProjectStore.branches.flatMap((b) =>
      b.folders.map((f) => ({ key: `${b.id}/${f.id}`, label: `${b.label} / ${f.label}` })),
    ),
  );

  function gaps(obj: InventoryObject): string[] {
    return slots.filter((s) => (obj.coverage[s.key] ?? 0) === 0).map((s) => s.label);
  }
</script>

<PanelShell title="Inventory" count={picusProjectStore.inventory.length}>
  {#snippet icon()}<Layers size={13} />{/snippet}

  {#snippet actions()}
    <Button
      variant="icon"
      size="xs"
      title="Re-index the project"
      ariaLabel="Re-index the project"
      onclick={() => toastStore.show('Project re-indexed.', 'success')}
    >
      {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
    </Button>
  {/snippet}

  {#snippet toolbar()}
    <SearchBar bind:query showRegex={false} placeholder="Filter objects" ariaLabel="Filter objects" />
  {/snippet}

  {#if !visible.length}
    <StateBlock
      tone="info"
      fill={false}
      label={picusProjectStore.inventory.length ? `Nothing matches “${query}”.` : 'Nothing indexed yet.'}
    />
  {:else}
    {#each visible as obj (obj.name)}
      {@const missing = gaps(obj)}
      <SidebarItem onclick={() => picusTabsStore.openInventory()}>
        {#snippet icon()}
          {#if obj.kind === 'table'}<Table2 size={13} />{:else}<Package size={13} />{/if}
        {/snippet}
        <span class="ip-name">{obj.name}</span>
        {#snippet badges()}
          {#if missing.length}
            <span
              class="ip-gap"
              use:tooltip={{
                content: `Missing from ${missing.length} location${missing.length === 1 ? '' : 's'}`,
                description: missing.join(' · '),
              }}
            >
              <TriangleAlert size={11} />
            </span>
          {/if}
        {/snippet}
      </SidebarItem>
    {/each}

    <p class="ip-hint">
      Coverage is compared across branches: Oracle and PostgreSQL have to tell the same
      story. Open the Inventory tab for the full matrix.
    </p>
  {/if}
</PanelShell>

<style>
  .ip-name {
    font-family: var(--font-code);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ip-gap { display: inline-flex; color: var(--error); }

  .ip-hint {
    padding: 10px 12px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-muted);
  }
</style>
