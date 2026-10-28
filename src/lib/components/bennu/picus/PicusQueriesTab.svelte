<script lang="ts">
  /**
   * Native queries vs. the database — each `@Query(nativeQuery = true)` (and its `countQuery`) is
   * prepared on the server, never executed, and the server's own rejection is the finding.
   */
  import { Play } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuPicusStore, nativeKey } from '$lib/stores/bennu/picus.svelte';
  import { openAt } from './open-at';
  import PicusOutcomeCell from './PicusOutcomeCell.svelte';

  let query = $state('');

  // The list is as the project is now, not as it was at the last refresh.
  $effect(() => {
    void bennuPicusStore.refreshModel();
  });

  const queries = $derived(bennuPicusStore.model?.native_queries ?? []);
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q
      ? queries.filter((s) => `${s.repository}.${s.method}`.toLowerCase().includes(q) || s.text.toLowerCase().includes(q))
      : queries;
  });
  const anyChecking = $derived(Object.values(bennuPicusStore.native).some((o) => o.state === 'checking'));
</script>

{#if queries.length === 0}
  <EmptyState message="No native queries." description="Spring Data @Query methods with nativeQuery = true appear here." />
{:else}
  <div class="tools">
    <SearchBar bind:query showRegex={false} showCounter={false} placeholder="Filter queries…" ariaLabel="Filter queries" />
  </div>
  <div class="bar">
    <span class="count">{shown.length === queries.length ? queries.length : `${shown.length} of ${queries.length}`} native {queries.length === 1 ? 'query' : 'queries'}</span>
    <Button size="xs" variant="tonal" disabled={anyChecking} onclick={() => void bennuPicusStore.checkAllNative()}>
      {#snippet iconStart()}<Play size={11} />{/snippet}
      Validate all
    </Button>
  </div>
  <ul class="rows">
    {#each shown as q (nativeKey(q))}
      {@const outcome = bennuPicusStore.native[nativeKey(q)]}
      <li>
        <div class="row">
          <button class="name" type="button" use:tooltip={`${q.repository}.${q.method}`} onclick={() => openAt(q.file, q.line)}>
            <span class="cls">{q.repository}.{q.method}</span>
            <span class="sql">{q.text.replace(/\s+/g, ' ').trim()}</span>
          </button>
          <PicusOutcomeCell {outcome} oncheck={() => void bennuPicusStore.checkNative(q)} />
        </div>
        {#if outcome?.state === 'rejected'}
          <ul class="msgs">{#each outcome.messages as m, i (i)}<li>{m}</li>{/each}</ul>
        {:else if outcome?.state === 'unchecked'}
          <ul class="msgs"><li>{outcome.reason}</li></ul>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .tools { padding: 8px 12px 0; }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 6px 12px; }
  .count { font-size: 11px; color: var(--text-muted); }
  .rows, .msgs { list-style: none; margin: 0; padding: 0; }
  .row { display: flex; align-items: center; gap: 8px; padding: 3px 8px 3px 12px; min-width: 0; }
  .row:hover { background: var(--bg-hover); }
  /* `flex: 1 1 0` with `min-width: 0`: the name takes what the verdict leaves and ellipsizes, instead of
     pushing the row wider than the panel when a repository method is long. */
  .name { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; background: none; border: none; padding: 0; text-align: left; cursor: pointer; color: inherit; }
  .cls { font-size: 12px; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .sql { font-size: 10.5px; color: var(--text-muted); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .msgs { padding: 0 12px 4px 12px; }
  .msgs li { font-size: 11px; line-height: 1.5; color: var(--danger, var(--text-secondary)); white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
