<script lang="ts">
  /**
   * One dependency nobody declared: it is here because something that was declared needs it.
   *
   * The row says who in a line ("via spring-web +2"); opened, it lists every route from what the
   * project asked for down to this artifact, so "why is this on my classpath" has an answer you can
   * follow. Routes come from the resolved tree, so before it is resolved the row only says so.
   */
  import { ChevronRight, Link2 } from 'lucide-svelte';
  import { collapse } from '$lib/utils/collapse';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { coordOf, type Dependency } from '$lib/ipc/bennu/deps';
  import { introducers, routes, type ArtifactUsage } from './dependency-tree';
  import { splitCoord } from './dep-labels';

  interface Props {
    dep: Dependency;
    /** What the resolved tree says about it; undefined before the tree is resolved. */
    usage?: ArtifactUsage;
    vuln?: { count: number; severity: string };
    /** The panel's filter, lower-cased: a route that matches it is shown open. */
    query?: string;
  }

  let { dep, usage, vuln, query = '' }: Props = $props();

  const coord = $derived(coordOf(dep));
  const parts = $derived(splitCoord(coord));
  const via = $derived.by(() => {
    const names = introducers(usage).map((c) => splitCoord(c).name);
    return names.length > 3 ? `${names.slice(0, 3).join(', ')} +${names.length - 3}` : names.join(', ');
  });
  const paths = $derived(routes(usage).map((route) => route.map((c) => splitCoord(c).name)));
  let open = $state(false);

  // A filter that hits a route (not just the name) is a filter whose answer is inside the row.
  $effect(() => {
    if (query && paths.some((p) => p.join(' ').toLowerCase().includes(query))) open = true;
  });
</script>

<div class="tr-row" class:open>
  <div class="tr-main" use:tooltip={dep.resolved || 'Not in the local cache'}>
    <span class="tr-top">
      <span class="tr-name">{parts.name}</span>
      <span class="tr-ver mono" class:conflict={usage?.conflicted}>{dep.version}</span>
    </span>
    <span class="tr-meta">
      {#if parts.group}<span class="tr-group mono">{parts.group}</span>{/if}
      {#if via}<span class="tr-via"><Link2 size={10} /> via {via}</span>{/if}
      {#if usage?.conflicted}
        <Badge variant="tone" tone="warning" size="sm" label={`${usage.versions.length} versions`} />
      {/if}
      {#if vuln}
        <span use:tooltip={`${vuln.count} known ${vuln.count === 1 ? 'vulnerability' : 'vulnerabilities'} (${vuln.severity})`}>
          <Badge variant="tone" tone="error" size="sm" label={`${vuln.count} CVE`} />
        </span>
      {/if}
    </span>
  </div>
  {#if paths.length > 0}
    <span class="chev" class:rot={open}>
      <IconButton tooltip={open ? 'Hide who requires it' : 'Show who requires it'} size={20} onclick={() => (open = !open)}>
        <ChevronRight size={12} />
      </IconButton>
    </span>
  {/if}
  {#if open}
    <div transition:collapse class="detail">
      <div class="d-head">Required by</div>
      <ul class="d-list">
        {#each paths as path (path.join('>'))}
          <li class="mono">
            {#each path.slice(0, -1) as step, i (i)}<span>{step}</span><span class="d-arrow">→</span>{/each}<span class="d-self">{path[path.length - 1]}</span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</div>

<style>
  .tr-row { position: relative; border-radius: var(--radius-sm); padding: 6px 28px 6px 8px; }
  .tr-row:hover { background: var(--bg-hover); }
  .tr-main { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .tr-top { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .tr-name {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--text-primary); font-size: var(--font-size-sm); font-weight: 500;
  }
  .tr-ver { flex-shrink: 0; color: var(--text-secondary); }
  .tr-ver.conflict { color: var(--warning); font-weight: 600; }
  .tr-meta { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; min-width: 0; }
  .tr-group { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tr-via {
    display: inline-flex; align-items: center; gap: 3px;
    font-size: var(--font-size-3xs); color: var(--text-muted);
  }
  .chev { position: absolute; top: 5px; right: 4px; transition: transform var(--transition-fast); }
  .chev.rot { transform: rotate(90deg); }

  .detail { padding: 2px 0 4px 10px; border-left: 1px solid var(--border-subtle); margin: 4px 0 0 4px; }
  .d-head {
    margin: 2px 0 3px; font-size: var(--font-size-3xs); text-transform: uppercase;
    letter-spacing: 0.5px; color: var(--text-muted); font-weight: 700;
  }
  .d-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 3px; }
  .d-list li { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px; min-width: 0; color: var(--text-secondary); }
  .d-arrow { color: var(--text-disabled); }
  .d-self { color: var(--text-primary); font-weight: 600; }
  .mono { font-family: var(--font-code); font-size: var(--font-size-xs); }
</style>
