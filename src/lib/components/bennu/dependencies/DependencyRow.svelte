<script lang="ts">
  /**
   * One declared dependency of a module.
   *
   * Name first, group beneath it: a coordinate read as one mono string buries the half you scan
   * for (the artifact), and `org.springframework.boot:spring-boot-starter-web` is the same length
   * wherever you look. The right edge carries the version — tinted when the resolved tree shows
   * more than one — and beneath the name sit the facts that change what the row means: scope,
   * where the version was decided, whether it resolved.
   *
   * Once the tree has been resolved the row can also open, to show what the dependency drags in
   * and, for a conflicted one, which versions the project saw and who asked for each. The click on
   * the row still goes to the declaration; the chevron is a sibling, never nested.
   */
  import { ChevronRight, GitFork, CircleSlash, TriangleAlert, ShieldAlert } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { Dependency, TreeNode } from '$lib/ipc/bennu/deps';
  import { coordOf, scopeLabel } from '$lib/ipc/bennu/deps';
  import type { ArtifactUsage } from './dependency-tree';
  import { nodeCoord } from './dependency-tree';
  import { originLabel, originTooltip, scopeTone, splitCoord } from './dep-labels';

  interface Props {
    dep: Dependency;
    ecosystem: string;
    /** Whether "not resolved" means anything yet (see `DependencyReport.resolved_known`). */
    resolvedKnown: boolean;
    /** What the resolved tree says about this artifact; undefined before the tree is resolved. */
    usage?: ArtifactUsage;
    /** What this dependency drags in, one level. */
    pulls?: TreeNode[];
    /** Known vulnerabilities of this artifact at its version, when there are any. */
    vuln?: { count: number; severity: string };
    onOpen: (d: Dependency) => void;
  }

  let { dep, ecosystem, resolvedKnown, usage, pulls = [], vuln, onOpen }: Props = $props();

  /** OSV says `moderate`; the severity tokens say `medium`. */
  const vulnColor = $derived(
    `var(--severity-${vuln?.severity.toLowerCase() === 'moderate' ? 'medium' : (vuln?.severity.toLowerCase() ?? 'unknown')})`,
  );

  const isCargo = $derived(ecosystem === 'cargo');
  const coord = $derived(coordOf(dep));
  const parts = $derived(splitCoord(coord));
  const unresolved = $derived(resolvedKnown && !dep.resolved);
  const conflicted = $derived(!!usage?.conflicted);
  const expandable = $derived(conflicted || pulls.length > 0);
  let open = $state(false);
</script>

<div class="row" class:unresolved class:open>
  <button type="button" class="main" onclick={() => onOpen(dep)} use:tooltip={originTooltip(dep, isCargo)}>
    <span class="top">
      <span class="name">{parts.name}</span>
      <span class="version mono" class:unknown={!dep.version} class:conflict={conflicted}>
        {dep.version || 'version unknown'}
      </span>
    </span>
    <span class="meta">
      {#if parts.group}<span class="group mono">{parts.group}</span>{/if}
      <span use:tooltip={`${scopeLabel(ecosystem)}: ${dep.scope}`}>
        <Badge variant="tone" tone={scopeTone(dep.scope)} size="sm" label={dep.scope} />
      </span>
      {#if dep.kind}<Badge variant="tone" tone="neutral" size="sm" label={dep.kind} />{/if}
      {#if dep.variant}
        <Badge variant="tone" tone="neutral" size="sm" label={isCargo ? `= ${dep.variant}` : dep.variant} />
      {/if}
      {#if dep.source && dep.source !== 'crates.io'}<Badge variant="tone" tone="neutral" size="sm" label={dep.source} />{/if}
      {#if dep.optional}<Badge variant="tone" tone="neutral" size="sm" label="optional" />{/if}
      {#if dep.condition}
        <Badge variant="tone" tone="neutral" size="sm" label={isCargo ? dep.condition : `profile: ${dep.condition}`} />
      {/if}
      {#if dep.features.length}<Badge variant="tone" tone="neutral" size="sm" label={`+${dep.features.join(' +')}`} />{/if}
      {#if dep.origin.kind !== 'declared'}
        <span class="note"><GitFork size={10} /><span class="note-txt">{originLabel(dep)}</span></span>
      {/if}
      {#if conflicted}
        <span class="note warn"><TriangleAlert size={10} />{usage?.versions.length} versions</span>
      {/if}
      {#if unresolved}<span class="note warn"><CircleSlash size={10} /> not resolved</span>{/if}
      {#if vuln}
        <span use:tooltip={`${vuln.count} known ${vuln.count === 1 ? 'vulnerability' : 'vulnerabilities'} (${vuln.severity}) — see Vulnerabilities`}>
          <Badge variant="chip" size="sm" color={vulnColor}>
            {#snippet icon()}<ShieldAlert size={9} />{/snippet}
            {vuln.count}
          </Badge>
        </span>
      {/if}
    </span>
  </button>
  {#if expandable}
    <span class="chev" class:rot={open}>
      <IconButton tooltip={open ? 'Hide details' : 'Show versions and what it pulls in'} size={20} onclick={() => (open = !open)}>
        <ChevronRight size={12} />
      </IconButton>
    </span>
  {/if}

  {#if open}
    <div class="detail">
      {#if conflicted && usage}
        <div class="d-head">Versions in the resolved tree</div>
        <ul class="d-list">
          {#each usage.versions as v, i (v)}
            <li>
              <span class="mono">{v}</span>
              {#if !isCargo}<span class="d-tag">{i === 0 ? 'kept' : 'dropped'}</span>{/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if pulls.length > 0}
        <div class="d-head">Pulls in</div>
        <ul class="d-list">
          {#each pulls.slice(0, 12) as n (nodeCoord(n) + n.version)}
            <li><span class="mono">{nodeCoord(n)}</span><span class="d-ver mono">{n.version}</span></li>
          {/each}
          {#if pulls.length > 12}<li class="d-more">…and {pulls.length - 12} more</li>{/if}
        </ul>
      {/if}
    </div>
  {/if}
</div>

<style>
  .row { position: relative; border-radius: var(--radius-sm); }
  .row:hover { background: var(--bg-hover); }
  .main {
    width: 100%; display: flex; flex-direction: column; gap: 4px; align-items: stretch;
    padding: 6px 28px 6px 8px; border: 0; background: none; text-align: left; cursor: pointer;
    color: inherit; font: inherit; border-radius: var(--radius-sm);
  }
  .main:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }
  .chev { position: absolute; top: 5px; right: 4px; transition: transform var(--transition-fast); }
  .chev.rot { transform: rotate(90deg); }

  .top { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .name {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--text-primary); font-size: var(--font-size-sm); font-weight: 600;
  }
  .row.unresolved .name { color: var(--warning); }
  .version { flex-shrink: 0; color: var(--text-secondary); }
  .version.unknown { font-style: italic; color: var(--text-disabled); }
  .version.conflict { color: var(--warning); font-weight: 600; }

  .meta { display: flex; align-items: center; flex-wrap: wrap; gap: 5px; min-width: 0; }
  .group {
    color: var(--text-muted); max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .note {
    display: inline-flex; align-items: center; gap: 3px; min-width: 0;
    font-size: var(--font-size-3xs); color: var(--text-muted);
  }
  .note.warn { color: var(--warning); }
  .note :global(svg) { flex-shrink: 0; }
  .note-txt { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .detail { padding: 2px 10px 8px 18px; border-left: 1px solid var(--border-subtle); margin-left: 12px; }
  .d-head {
    margin: 6px 0 3px; font-size: var(--font-size-3xs); text-transform: uppercase;
    letter-spacing: 0.5px; color: var(--text-muted); font-weight: 700;
  }
  .d-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .d-list li { display: flex; align-items: baseline; gap: 8px; min-width: 0; font-size: var(--font-size-xs); }
  .d-ver { margin-left: auto; color: var(--text-muted); }
  .d-tag { color: var(--text-muted); font-size: var(--font-size-3xs); }
  .d-more { color: var(--text-disabled); }
  .mono { font-family: var(--font-code); font-size: var(--font-size-xs); }
</style>
