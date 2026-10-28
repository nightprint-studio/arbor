<script lang="ts">
  /**
   * Entities vs. the database — one row per `@Entity`, with what the schema says about it.
   *
   * Problems expand under the row they belong to and jump to the field's line, so the loop is
   * "read the finding, land on the field, fix it, refresh". A column no field maps is a finding of its
   * own — "missing" from the entity — listed after the real problems and dimmed, because a legacy
   * table has columns an entity never needed and that is information rather than an error.
   *
   * 80 entities is a list nobody scrolls: the search narrows it by class or table, the filter by what
   * the check found.
   */
  import { ChevronDown, ChevronRight } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuPicusStore } from '$lib/stores/bennu/picus.svelte';
  import type { EntityCheck } from './entity-check';
  import { openAt } from './open-at';

  type Filter = 'all' | 'problems' | 'unmapped' | 'ok';

  // Shown means current: the entities are read again as they are now, not as the window last saw them.
  $effect(() => {
    void bennuPicusStore.refreshModel();
  });

  let open = $state<Record<string, boolean>>({});
  let query = $state('');
  let filter = $state<Filter>('all');

  const checks = $derived(bennuPicusStore.checks);
  const realProblems = (c: EntityCheck) => c.problems.filter((p) => p.kind !== 'unmapped-column').length;
  const unmapped = (c: EntityCheck) => c.problems.filter((p) => p.kind === 'unmapped-column').length;

  const counts = $derived({
    drift: checks.filter((c) => c.status === 'drift').length,
    missing: checks.filter((c) => c.status === 'missing-table').length,
    unmapped: checks.filter((c) => unmapped(c) > 0).length,
  });

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return checks.filter((c) => {
      if (q && !c.entity.simple.toLowerCase().includes(q) && !c.tableName.toLowerCase().includes(q)) return false;
      switch (filter) {
        case 'problems': return c.status !== 'ok';
        case 'unmapped': return unmapped(c) > 0;
        case 'ok': return c.status === 'ok';
        default: return true;
      }
    });
  });

  function tone(c: EntityCheck) {
    return c.status === 'ok' ? 'success' : c.status === 'drift' ? 'warning' : 'error';
  }
  function label(c: EntityCheck) {
    if (c.status === 'ok') return 'matches';
    if (c.status === 'missing-table') return 'no table';
    return `${realProblems(c)} issue${realProblems(c) === 1 ? '' : 's'}`;
  }
</script>

{#if checks.length === 0}
  <EmptyState message="No entities to check." description="Refresh once a connection is open, or this project has no @Entity classes." />
{:else}
  <div class="tools">
    <SearchBar bind:query showRegex={false} showCounter={false} placeholder="Filter entities or tables…" ariaLabel="Filter entities" />
    <Select fill size="sm" ariaLabel="Show" value={filter} onchange={(v) => (filter = v as Filter)}
      highlight={filter !== 'all'}
      options={[
        { value: 'all', label: `All (${checks.length})` },
        { value: 'problems', label: `With problems (${counts.drift + counts.missing})` },
        { value: 'unmapped', label: `With unmapped columns (${counts.unmapped})` },
        { value: 'ok', label: 'Matching' },
      ]} />
  </div>
  <p class="summary">
    {shown.length} of {checks.length}
    {#if counts.drift || counts.missing}— {counts.drift} drifting, {counts.missing} without a table{:else}— all match{/if}
  </p>
  {#if shown.length === 0}
    <EmptyState message="Nothing matches." description="Change the filter or the search." />
  {:else}
    <ul class="rows">
      {#each shown as c (c.entity.fqcn)}
        {@const expandable = c.problems.length > 0}
        <li>
          <div class="row">
            <button class="chev" type="button" disabled={!expandable} aria-expanded={!!open[c.entity.fqcn]}
              aria-label="Show findings" onclick={() => (open[c.entity.fqcn] = !open[c.entity.fqcn])}>
              {#if expandable}
                {#if open[c.entity.fqcn]}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
              {/if}
            </button>
            <button class="name" type="button" use:tooltip={`${c.entity.fqcn} → ${c.tableName}`}
              onclick={() => openAt(c.entity.file, c.entity.line)}>
              <span class="cls">{c.entity.simple}</span>
              <span class="tbl">{c.tableName}</span>
            </button>
            <span class="badges">
              {#if unmapped(c) > 0}
                <Badge variant="tone" tone="neutral" label={`+${unmapped(c)} unmapped`} />
              {/if}
              <Badge variant="tone" tone={tone(c)} label={label(c)} />
            </span>
          </div>
          {#if open[c.entity.fqcn]}
            <ul class="problems">
              {#each c.problems as p, i (i)}
                <li class:info={p.kind === 'unmapped-column'}>
                  {#if p.line}
                    <button type="button" class="jump" onclick={() => openAt(c.entity.file, p.line!)}>{p.message}</button>
                  {:else}
                    <span>{p.message}</span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
{/if}

<style>
  .tools { display: flex; flex-direction: column; gap: 6px; padding: 8px 12px 0; }
  .summary { margin: 0; padding: 6px 12px; font-size: 11px; color: var(--text-muted); }
  .rows, .problems { list-style: none; margin: 0; padding: 0; }
  .row { display: flex; align-items: center; gap: 4px; padding: 3px 8px 3px 4px; min-width: 0; }
  .row:hover { background: var(--bg-hover); }
  .chev { width: 18px; flex: none; display: grid; place-items: center; background: none; border: none; color: var(--text-muted); padding: 0; cursor: pointer; }
  .chev:disabled { cursor: default; }
  .name { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; gap: 0; background: none; border: none; padding: 0; text-align: left; cursor: pointer; color: inherit; }
  .cls { font-size: 12px; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .tbl { font-size: 10.5px; color: var(--text-muted); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  /* The status never shrinks: a long class name gives way to an ellipsis, not the badge to a sliver. */
  .badges { flex: none; display: flex; align-items: center; gap: 4px; }
  .problems { padding: 0 8px 4px 30px; }
  .problems li { font-size: 11px; line-height: 1.5; color: var(--text-secondary); overflow-wrap: anywhere; }
  .problems li.info { color: var(--text-muted); }
  .jump { background: none; border: none; padding: 0; text-align: left; color: inherit; cursor: pointer; font: inherit; overflow-wrap: anywhere; }
  .jump:hover { color: var(--text-primary); text-decoration: underline; }
</style>
