<script lang="ts">
  /**
   * Inventory panel — every object the scripts define or touch, and whether the
   * repository tells the same story about it everywhere.
   *
   * "Everywhere" is not "in every folder": coverage arrives keyed by folder path
   * and a repository has hundreds of those, most of them one delivered version
   * of the same thing. It is folded onto the axes the rules actually compare —
   * **engine × role** — by `utils/picus/coverage.ts`. A zero in any of those
   * columns is a gap: something exists in Oracle and not in PostgreSQL, or in
   * initialisation and not in updates. That is the `CONS001`/`CONS002` family,
   * surfaced here as a marker before the analysis has even been asked for.
   */
  import { Layers, RefreshCw, TriangleAlert } from 'lucide-svelte';
  import ObjectKindIcon from '../PicusObjectKindIcon.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import NoticeList from './NoticeList.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { coverageBuckets, coverageGaps } from '$lib/utils/picus/coverage';

  let query = $state('');
  const needle = $derived(query.trim().toLowerCase());

  const visible = $derived(
    picusProjectStore.inventory.filter((o) => !needle || o.name.toLowerCase().includes(needle)),
  );

  /** The comparison axes this repository has — engine × role, never per folder. */
  const buckets = $derived(coverageBuckets(picusProjectStore.tree));
</script>

<PanelShell title="Inventory" count={picusProjectStore.inventory.length}>
  {#snippet icon()}<Layers size={13} />{/snippet}

  {#snippet actions()}
    <Button
      variant="icon"
      size="xs"
      tooltip={{ content: 'Re-index and re-check the repository', shortcut: 'Ctrl+Shift+K' }}
      ariaLabel="Re-index the repository"
      disabled={!picusProjectStore.attached || picusProjectStore.analyzing}
      onclick={() => void picusProjectStore.analyze()}
    >
      {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
    </Button>
  {/snippet}

  {#snippet toolbar()}
    <SearchBar bind:query showRegex={false} placeholder="Filter objects" ariaLabel="Filter objects" />
  {/snippet}

  {#if !picusProjectStore.attached}
    <StateBlock
      tone="info"
      fill={false}
      label="No repository attached to this connection — there is nothing to index."
    />
  {:else if picusProjectStore.analysisError}
    <div class="ip-error">
      <Alert variant="error" compact title="The index could not be built" text={picusProjectStore.analysisError} />
    </div>
  {:else if picusProjectStore.analyzing && !picusProjectStore.inventory.length}
    <StateBlock tone="loading">
      {#snippet spinner()}<Spinner size={14} />{/snippet}
      <span>Indexing the repository…</span>
    </StateBlock>
  {:else if !visible.length}
    <StateBlock
      tone="info"
      fill={false}
      label={picusProjectStore.inventory.length ? `Nothing matches “${query}”.` : 'Nothing indexed yet.'}
    />
  {:else}
    <!-- Keyed by kind AND name: a name alone is not unique. The same identifier
         can be indexed under two kinds — a view for one engine that another writes
         into as a table, a package spec beside something of its own name — and a
         duplicate key is a hard Svelte error that takes the whole panel down
         rather than merely drawing one row twice. -->
    {#each visible as obj (`${obj.kind}/${obj.name}`)}
      {@const missing = coverageGaps(obj, buckets)}
      <SidebarItem onclick={() => picusTabsStore.openInventory()}>
        {#snippet icon()}
          <ObjectKindIcon kind={obj.kind} />
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

    <!-- Something the index found that no classified folder claims. Not a gap
         between two engines — a place outside the model altogether, worth naming. -->
    <NoticeList notes={picusProjectStore.orphans} label="Outside every classified folder" />

    <p class="ip-hint">
      Coverage is compared by engine and by role: Oracle and PostgreSQL have to tell the
      same story, and so do the updates and the initialisation. Open the Inventory tab for
      the matrix and the folder-by-folder detail.
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

  .ip-error { padding: 8px 12px; }
</style>
