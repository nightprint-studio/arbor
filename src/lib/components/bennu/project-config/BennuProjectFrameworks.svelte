<script lang="ts">
  /**
   * Project Configuration › Frameworks — what Bennu recognised in this project, what convinced it,
   * and — for what it did not recognise — what it would have taken.
   *
   * Read-only, and that is the point: these are facts about the tree, not preferences. They decide
   * how much of Bennu applies — a Struts project gets the action navigation, a Bean Validation one
   * gets the DTO lab — so the screen that answers "why is that feature not here" is this one.
   *
   * ## What changed from a list of what was found
   *
   * A list of detected capabilities cannot answer that question for the one that is *missing*. So
   * every capability the backend knows is here, grouped by kind, found or not: a found one shows its
   * strongest evidence and folds the individual signs under it; a missing one says what Bennu looks
   * for, which is the thing to add. "Show not detected" is on by default for that reason, and a
   * filter narrows the rest when the list is long.
   */
  import { Boxes, Circle, CircleCheck, TriangleAlert } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import BennuFilterBar from '../BennuFilterBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { GROUP_LABEL, GROUP_ORDER, infoOf, type CapabilityGroup } from './capability-catalog';

  const caps = $derived(projectStore.capabilities);
  const hits = $derived(caps?.hits ?? []);

  let query = $state('');
  let showMissing = $state(true);

  /** What a tier means, said once where it is read rather than in a manual. */
  const TIER_MEANS: Record<string, string> = {
    A: 'A dependency — the strongest evidence there is',
    B: 'A configuration file of the framework',
    C: 'A pattern in the source — corroborating, and provisional on its own',
  };

  interface Row {
    key: string;
    label: string;
    group: CapabilityGroup;
    what: string;
    looksFor: string;
    on: boolean;
    /** The strongest tier among its evidence, or '' when there is none. */
    best: string;
    /** Only source-pattern evidence — Bennu says so, because it can be wrong. */
    provisional: boolean;
    hits: typeof hits;
  }

  /** Every boolean the backend reported, joined with its evidence and the catalogue. Built from the
   *  set itself, not from the catalogue, so a capability added on the Rust side appears at once. */
  const rows = $derived.by<Row[]>(() => {
    if (!caps) return [];
    return Object.entries(caps)
      .filter(([, v]) => typeof v === 'boolean')
      .map(([key, v]) => {
        const info = infoOf(key);
        const mine = hits.filter((h) => h.capability === key);
        const tiers = [...new Set(mine.map((h) => h.tier))].sort();
        return {
          key,
          label: info.label,
          group: info.group,
          what: info.what,
          looksFor: info.looksFor,
          on: v === true,
          best: tiers[0] ?? '',
          provisional: tiers.length > 0 && tiers.every((t) => t === 'C'),
          hits: mine,
        };
      });
  });

  const foundCount = $derived(rows.filter((r) => r.on).length);

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return rows.filter(
      (r) =>
        (showMissing || r.on) &&
        (!q || `${r.label} ${r.what} ${r.looksFor} ${r.hits.map((h) => h.detail).join(' ')}`.toLowerCase().includes(q)),
    );
  });

  /** Groups in a fixed order; within one, what was found first (strongest evidence first), then the
   *  rest by name. */
  const groups = $derived(
    GROUP_ORDER.map((g) => ({
      group: g,
      rows: visible
        .filter((r) => r.group === g)
        .sort((a, b) =>
          Number(b.on) - Number(a.on) || (a.best || 'Z').localeCompare(b.best || 'Z') || a.label.localeCompare(b.label)),
    })).filter((g) => g.rows.length > 0),
  );
</script>

<div class="section-header">
  <h2>Frameworks</h2>
  <p>
    What Bennu recognised here, what convinced it, and what it looks for in the ones it did not find.
    Facts about the tree, not preferences — they decide which of Bennu’s features this project gets.
  </p>
</div>

{#if rows.length === 0}
  <EmptyState message="No capability information for this project." />
{:else}
  <div class="toolbar">
    <span class="summary" aria-live="polite">
      <strong>{foundCount}</strong> detected · {rows.length - foundCount} not found
    </span>
    <div class="filter"><BennuFilterBar bind:query placeholder="Filter frameworks…" /></div>
    <Toggle bind:checked={showMissing} size="sm" label="Show not detected" />
  </div>

  {#if groups.length === 0}
    <EmptyState message={query ? 'Nothing matches the filter.' : 'No domain frameworks detected in this project.'} compact />
  {/if}

  {#each groups as g (g.group)}
    <div class="card">
      <div class="card-section-title">
        <Boxes size={12} /> {GROUP_LABEL[g.group]}
        <span class="g-count">{g.rows.filter((r) => r.on).length}/{g.rows.length}</span>
      </div>
      <div class="rows">
        {#each g.rows as r (r.key)}
          {#snippet head()}
            <span class="row-head">
              <span class="state" class:on={r.on} use:tooltip={r.on ? 'Detected' : 'Not detected'}>
                {#if r.on}<CircleCheck size={15} />{:else}<Circle size={15} />{/if}
              </span>
              <span class="text">
                <span class="name" class:off={!r.on}>{r.label}</span>
                <span class="desc">{r.on ? r.what : r.looksFor ? `Looks for ${r.looksFor}` : r.what}</span>
              </span>
              {#if r.on}
                {#if r.provisional}
                  <span class="prov" use:tooltip={'Only a pattern in the source supports this — it can be wrong'}>
                    <TriangleAlert size={11} /> provisional
                  </span>
                {/if}
                {#if r.best}
                  <span class="tier tier-{r.best.toLowerCase()}" use:tooltip={TIER_MEANS[r.best] ?? ''}>{r.best}</span>
                {/if}
                <span class="signs">{r.hits.length} {r.hits.length === 1 ? 'sign' : 'signs'}</span>
              {/if}
            </span>
          {/snippet}

          {#if r.on && r.hits.length > 0}
            <Collapsible chevron header={head}>
              <div class="evidence">
                {#if r.looksFor}<p class="also">Looks for {r.looksFor}</p>{/if}
                <ul>
                  {#each r.hits as h, i (i)}
                    <li>
                      <span class="tier tier-{h.tier.toLowerCase()}" use:tooltip={TIER_MEANS[h.tier] ?? ''}>{h.tier}</span>
                      <span class="detail">{h.detail}</span>
                    </li>
                  {/each}
                </ul>
              </div>
            </Collapsible>
          {:else}
            <div class="row-static">
              {@render head()}
              {#if r.on}
                <p class="also">Detected from the project layout rather than from a dependency or a file.</p>
              {/if}
            </div>
          {/if}
        {/each}
      </div>
    </div>
  {/each}

  <p class="legend">
    <span class="tier tier-a">A</span> dependency
    <span class="tier tier-b">B</span> configuration file
    <span class="tier tier-c">C</span> source pattern
  </p>
{/if}

<style>
  .toolbar { display: flex; align-items: center; gap: 12px; margin: 4px 0 12px; }
  .summary { font-size: var(--font-size-xs); color: var(--text-muted); white-space: nowrap; }
  .summary strong { color: var(--text-primary); }
  .filter { flex: 1; min-width: 0; }

  .g-count { margin-left: 6px; color: var(--text-muted); font-weight: 500; font-variant-numeric: tabular-nums; }

  .rows { display: flex; flex-direction: column; padding: 2px 10px 8px; }
  .row-static { padding: 2px 0; }
  .row-head { display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0; padding: 5px 0; }

  .state { display: inline-flex; flex-shrink: 0; color: var(--text-disabled); }
  .state.on { color: var(--success); }
  .text { display: flex; flex-direction: column; gap: 1px; flex: 1; min-width: 0; }
  .name { font-size: var(--font-size-sm); color: var(--text-primary); font-weight: 500; }
  .name.off { color: var(--text-secondary); font-weight: 400; }
  .desc { font-size: var(--font-size-2xs); color: var(--text-muted); line-height: 1.4; }

  .signs { font-size: var(--font-size-2xs); color: var(--text-muted); white-space: nowrap; }
  .prov {
    display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0;
    font-size: var(--font-size-3xs); color: var(--warning);
  }

  .evidence { padding: 0 0 8px 25px; }
  .also { margin: 2px 0 6px; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .row-static .also { padding-left: 25px; }
  .evidence ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .evidence li { display: flex; align-items: center; gap: 8px; }
  .detail {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .tier {
    flex-shrink: 0; width: 18px; height: 18px; border-radius: var(--radius-sm);
    display: inline-flex; align-items: center; justify-content: center;
    font-size: var(--font-size-2xs); font-weight: 700;
  }
  .tier-a { color: var(--success); background: color-mix(in srgb, var(--success) 18%, transparent); }
  .tier-b { color: var(--info);    background: color-mix(in srgb, var(--info) 18%, transparent); }
  .tier-c { color: var(--warning); background: color-mix(in srgb, var(--warning) 18%, transparent); }

  .legend {
    display: flex; align-items: center; flex-wrap: wrap; gap: 6px 14px; margin: 10px 2px 0;
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .legend .tier { margin-right: -8px; }
</style>
