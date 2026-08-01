<script lang="ts">
  /**
   * The Note types sidebar section: what this vault's notes are, and how many of
   * each.
   *
   * The types are the vault's own declaration of its shape — they live inside it,
   * under `.arbor/garrulus/types/`, and travel with it — so this is the section
   * that answers "what is in here" without opening a single note. Each row is also
   * a filter: clicking Bug is `type:bug`, which is the same query the search box
   * would build, run through the same store, so the two can never disagree about
   * what is being filtered.
   *
   * **The counts are searches.** There is no facet endpoint; a filter-only query
   * selects exactly the notes of that type, so the count is that query's length
   * (see `facets.ts`). One round trip per type, on mount and on demand — reads,
   * never writes, and never on a timer.
   *
   * Renders the body only: the host wraps it in the section's `PanelShell`, which
   * already carries the title.
   */
  import { untrack } from 'svelte';
  import { RefreshCw } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { garrulusSearchStore } from '$lib/stores/garrulus/search.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import { typeToken } from '../search/query-tokens';
  import { countMatching, mapWithLimit } from './facets';

  interface Props {
    /** Bring the search results on screen after a row has filtered them. Absent →
     *  the row still filters, and the host is assumed to be showing them already. */
    onShowResults?: () => void;
  }

  let { onShowResults }: Props = $props();

  /** How many counts to ask for at once. Each is a round trip; more than a
   *  handful in flight only moves the queue, it does not move the paint. */
  const IN_FLIGHT = 4;

  let counts = $state<Record<string, number | null>>({});
  let loading = $state(false);
  /** Discriminates a reload, so a slow one that was superseded cannot land. */
  let seq = 0;

  const types = $derived(garrulusVaultStore.types);

  export async function refresh(): Promise<void> {
    const list = types;
    if (list.length === 0) {
      counts = {};
      return;
    }
    const run = ++seq;
    loading = true;
    try {
      const found = await mapWithLimit(list, IN_FLIGHT, (t) =>
        countMatching(`type:${t.id}`),
      );
      if (run !== seq) return;
      const next: Record<string, number | null> = {};
      list.forEach((t, i) => (next[t.id] = found[i]));
      counts = next;
    } finally {
      if (run === seq) loading = false;
    }
  }

  /**
   * Count on arrival, and again when the vault's types change — which happens
   * when a vault opens, closes, or is re-indexed.
   *
   * A read, and the only thing that would make it wrong is not doing it: a
   * section showing the previous vault's numbers is worse than one showing none.
   */
  let lastKey: string | null = null;
  $effect(() => {
    const key = `${garrulusVaultStore.root ?? ''}::${types.map((t) => t.id).join(',')}`;
    if (key === lastKey) return;
    lastKey = key;
    untrack(() => void refresh());
  });

  function apply(typeId: string) {
    void garrulusSearchStore.toggleAndRun(typeToken(typeId));
    onShowResults?.();
  }
</script>

<div class="tp">
  <div class="tp-head">
    <span class="tp-subhead">Note types</span>
    <span class="tp-grow"></span>
    {#if loading}
      <Spinner size={11} />
    {:else}
      <Button
        variant="icon"
        size="xs"
        ariaLabel="Recount the notes of each type"
        tooltip={{ content: 'Recount. Reads the index — it changes nothing.' }}
        onclick={() => void refresh()}
      >
        {#snippet iconStart()}<RefreshCw size={11} />{/snippet}
      </Button>
    {/if}
  </div>

  {#if !garrulusVaultStore.isOpen}
    <EmptyState
      message="No vault open."
      description="Note types are declared inside a vault, so there are none to show until one is."
    />
  {:else if types.length === 0}
    <EmptyState
      message="This vault declares no note types."
      description="A type is a TOML file under .arbor/garrulus/types/ — it decides where its notes land, what they are called and what they start with."
    />
  {:else}
    <ul class="tp-list">
      {#each types as type (type.id)}
        {@const active = garrulusSearchStore.has(typeToken(type.id))}
        {@const count = counts[type.id]}
        <li>
          <button
            type="button"
            class="tp-row"
            class:active
            aria-pressed={active}
            title={active ? `Stop filtering by ${type.name}` : `Show every ${type.name}`}
            onclick={() => apply(type.id)}
          >
            <!-- The type's own accent, which is the one colour in this window
                 that means "kind" rather than "state". -->
            <span class="tp-dot" style="background: {type.accent}"></span>
            <span class="tp-name">{type.name}</span>
            {#if count !== null && count !== undefined}
              <span class="tp-pill">{count}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .tp { display: flex; flex-direction: column; min-height: 0; }

  .tp-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 6px 4px 10px;
  }
  .tp-subhead {
    font-size: var(--font-size-3xs);
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .tp-grow { flex: 1; }

  .tp-list { list-style: none; margin: 0; padding: 0; }

  .tp-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 24px;
    padding: 0 10px 0 12px;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    font-family: inherit;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .tp-row:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tp-row.active {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--text-primary);
  }

  .tp-dot {
    width: 7px;
    height: 7px;
    border-radius: 2px;
    flex: none;
  }
  .tp-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tp-pill {
    flex: none;
    padding: 0 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-size: var(--font-size-3xs);
    line-height: 15px;
  }
  .tp-row.active .tp-pill { color: var(--text-secondary); }
</style>
