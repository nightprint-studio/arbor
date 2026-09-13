<script lang="ts">
  /**
   * Project Configuration › Frameworks — what Bennu recognised in this project, and what convinced
   * it.
   *
   * Read-only, and that is the point: these are facts about the tree, not preferences. They decide
   * how much of Bennu applies — a Struts project gets the action navigation, a Bean Validation one
   * gets the DTO lab — so the screen that answers "why is that feature not here" is this one.
   *
   * One row per capability, its strongest evidence tier in front, and the individual signs folded
   * away underneath for the moment you disagree with the verdict.
   */
  import { Boxes } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';

  const caps = $derived(projectStore.capabilities);
  const enabledCaps = $derived.by(() => {
    if (!caps) return [] as string[];
    return Object.entries(caps).filter(([k, v]) => k !== 'hits' && v === true).map(([k]) => k);
  });
  const capHits = $derived(caps?.hits ?? []);

  function capLabel(field: string): string {
    return field.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  }

  /**
   * The evidence, grouped by what it activated.
   *
   * Flat, it was thirty lines in which the same capability appeared three times and the eye had to
   * do the grouping. A capability is one row now — its strongest tier, how many pieces of evidence
   * stand behind it — and the lines themselves are under it.
   */
  const capGroups = $derived.by(() => {
    const by = new Map<string, { capability: string; tiers: string[]; hits: typeof capHits }>();
    for (const hit of capHits) {
      const found = by.get(hit.capability) ?? { capability: hit.capability, tiers: [], hits: [] };
      found.hits = [...found.hits, hit];
      if (!found.tiers.includes(hit.tier)) found.tiers = [...found.tiers, hit.tier];
      by.set(hit.capability, found);
    }
    return [...by.values()]
      .map((g) => ({ ...g, best: [...g.tiers].sort()[0] ?? 'C' }))
      .sort((a, b) => a.best.localeCompare(b.best) || a.capability.localeCompare(b.capability));
  });

  /** What a tier means, said once where it is read rather than in a manual. */
  const TIER_MEANS: Record<string, string> = {
    A: 'A dependency — the strongest evidence there is',
    B: 'A configuration file of the framework',
    C: 'A pattern in the source — corroborating, and provisional on its own',
  };

  /** Capabilities that fired with no evidence line behind them — worth saying rather than hiding:
   *  it is how a detection that comes from somewhere else is told from one that was inferred. */
  const unexplained = $derived(
    enabledCaps.filter((c) => !capGroups.some((g) => g.capability === c)),
  );
</script>

<div class="section-header">
  <h2>Frameworks</h2>
  <p>
    What Bennu recognised here, and what convinced it. Facts about the tree, not preferences — they
    decide which of Bennu’s features this project gets.
  </p>
</div>

{#if enabledCaps.length === 0}
  <EmptyState message="No domain frameworks detected in this project." />
{:else}
  <div class="card">
    <div class="card-section-title"><Boxes size={12} /> Detected — {enabledCaps.length}</div>
    <div class="set-chips">
      {#each enabledCaps as c (c)}
        <Badge variant="tone" tone="accent" label={capLabel(c)} />
      {/each}
    </div>
  </div>

  {#if capGroups.length}
    <div class="card">
      <div class="card-section-title"><Boxes size={12} /> Evidence</div>
      <div class="groups">
        {#each capGroups as g (g.capability)}
          <Collapsible chevron>
            {#snippet header()}
              <span class="cap-head">
                <span class="tier tier-{g.best.toLowerCase()}" use:tooltip={TIER_MEANS[g.best] ?? ''}>{g.best}</span>
                <span class="cap-name">{capLabel(g.capability)}</span>
                <span class="cap-count">{g.hits.length} {g.hits.length === 1 ? 'sign' : 'signs'}</span>
              </span>
            {/snippet}
            <ul class="cap-evidence">
              {#each g.hits as h, i (i)}
                <li>
                  <span class="tier tier-{h.tier.toLowerCase()}" use:tooltip={TIER_MEANS[h.tier] ?? ''}>{h.tier}</span>
                  <span class="cap-detail">{h.detail}</span>
                </li>
              {/each}
            </ul>
          </Collapsible>
        {/each}
      </div>
      {#if unexplained.length}
        <p class="set-hint">
          No evidence line for {unexplained.map(capLabel).join(', ')} — detected from the project
          layout rather than from a dependency or a file.
        </p>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .groups { display: flex; flex-direction: column; padding: 4px 10px 8px; }
  .cap-head { display: flex; align-items: center; gap: 8px; flex: 1; min-width: 0; }
  .cap-name { font-size: var(--font-size-sm); color: var(--text-primary); flex: 1; min-width: 0; }
  .cap-count { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .cap-evidence { list-style: none; margin: 0; padding: 2px 0 6px 10px; display: flex; flex-direction: column; gap: 4px; }
  .cap-evidence li { display: flex; align-items: center; gap: 8px; }
  .cap-detail {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .tier {
    flex-shrink: 0; width: 18px; height: 18px; border-radius: var(--radius-sm);
    display: flex; align-items: center; justify-content: center;
    font-size: var(--font-size-2xs); font-weight: 700;
  }
  .tier-a { color: var(--success); background: color-mix(in srgb, var(--success) 18%, transparent); }
  .tier-b { color: var(--info);    background: color-mix(in srgb, var(--info) 18%, transparent); }
  .tier-c { color: var(--warning); background: color-mix(in srgb, var(--warning) 18%, transparent); }
</style>
