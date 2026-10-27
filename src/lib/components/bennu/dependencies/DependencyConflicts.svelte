<script lang="ts">
  /**
   * The artifacts the resolved tree sees at more than one version, and who asked for each.
   *
   * This is the question the plain list cannot answer: `commons-logging 1.2` is on the classpath,
   * but two things wanted different versions of it and Maven picked one. Each entry names the
   * version that won, the ones that lost, and for every version the chain of dependencies that
   * requested it — so "why is this here, and who do I bump" has an answer without running
   * `dependency:tree` by hand.
   */
  import { ChevronRight } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import type { ArtifactUsage, Occurrence } from './dependency-tree';
  import { splitCoord } from './dep-labels';

  let { conflicts, ecosystem = 'maven' }: { conflicts: ArtifactUsage[]; ecosystem?: string } = $props();

  /** Maven keeps one version and drops the rest; Cargo builds them all, so "kept / dropped" would
   *  be wrong there and the versions are listed plainly. */
  const mediates = $derived(ecosystem !== 'cargo');

  const open = $state<Record<string, boolean>>({});

  /** `a → b → c`, or where the module itself declares it. */
  function requester(o: Occurrence): string {
    if (o.chain.length === 0) return `declared by ${o.module.split(':').pop()}`;
    return o.chain.map((c) => splitCoord(c.coord).name).join(' → ');
  }

  /** For one version: distinct requesters, nearest-to-the-surface first. */
  function askedBy(usage: ArtifactUsage, version: string): string[] {
    const seen: string[] = [];
    for (const o of usage.occurrences) {
      if (o.node.version !== version) continue;
      const r = requester(o);
      if (!seen.includes(r)) seen.push(r);
    }
    return seen.sort((a, b) => a.length - b.length);
  }
</script>

<ul class="dc-list">
  {#each conflicts as usage (usage.coord)}
    {@const parts = splitCoord(usage.coord)}
    <li class="dc-item">
      <button
        type="button"
        class="dc-head"
        aria-expanded={open[usage.coord] ?? false}
        onclick={() => (open[usage.coord] = !open[usage.coord])}
      >
        <span class="dc-chev" class:rot={open[usage.coord]}><ChevronRight size={12} /></span>
        <span class="dc-name">{parts.name}</span>
        <span class="dc-versions">
          {#each usage.versions as v, i (v)}
            <Badge variant="tone" tone={!mediates ? 'neutral' : i === 0 ? 'success' : 'warning'} size="sm" label={v} />
          {/each}
        </span>
      </button>
      {#if parts.group}<div class="dc-group mono">{parts.group}</div>{/if}
      {#if open[usage.coord]}
        <div class="dc-body">
          {#each usage.versions as v, i (v)}
            <div class="dc-ver">
              <span class="mono dc-v">{v}</span>
              {#if mediates}<span class="dc-state" class:kept={i === 0}>{i === 0 ? 'kept' : 'dropped'}</span>{/if}
            </div>
            <ul class="dc-asked">
              {#each askedBy(usage, v).slice(0, 6) as r (r)}<li>{r}</li>{/each}
            </ul>
          {/each}
        </div>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .dc-list { list-style: none; margin: 0; padding: 0 6px 2px 0; display: flex; flex-direction: column; gap: 1px; }
  .dc-item { border-radius: var(--radius-sm); padding-bottom: 4px; }
  .dc-item:hover { background: var(--bg-hover); }
  .dc-head {
    width: 100%; display: flex; align-items: center; gap: 6px; padding: 6px 8px 2px;
    border: 0; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer;
  }
  .dc-head:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }
  .dc-chev { display: inline-flex; color: var(--text-muted); transition: transform var(--transition-fast); }
  .dc-chev.rot { transform: rotate(90deg); }
  .dc-name {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary);
  }
  .dc-versions { display: inline-flex; gap: 4px; flex-shrink: 0; }
  .dc-group { padding: 0 8px 0 26px; color: var(--text-muted); }
  .dc-body { padding: 4px 8px 2px 26px; }
  .dc-ver { display: flex; align-items: baseline; gap: 8px; margin-top: 4px; }
  .dc-v { color: var(--text-primary); }
  .dc-state { font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.4px; color: var(--warning); }
  .dc-state.kept { color: var(--success); }
  .dc-asked {
    list-style: none; margin: 2px 0 0; padding: 0 0 0 10px; border-left: 1px solid var(--border-subtle);
    display: flex; flex-direction: column; gap: 1px;
    font-size: var(--font-size-2xs); color: var(--text-secondary);
  }
  .mono { font-family: var(--font-code); font-size: var(--font-size-xs); }
</style>
