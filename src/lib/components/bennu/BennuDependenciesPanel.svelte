<script lang="ts">
  /**
   * Dependencies (left tool window) — the resolved dependency set for the open
   * Java project, grouped by MODULE. For each dependency it shows the
   * `groupId:artifactId` coordinate, the version, the scope (as a toned Badge) and
   * the ORIGIN: either declared directly by the module, or a subtle "from
   * <parent-artifactId>" tag when it's inherited from a parent pom's
   * `<dependencyManagement>` / parent inheritance.
   *
   * Single-module projects render one group; multi-module projects render one
   * SidebarSection per module (same grouping widget as the Maven / Structure
   * tools). A SearchBar filters by coordinate/version/scope, and the header shows
   * a total count.
   *
   * MOCK — the data comes from `dependencies.svelte.ts`; the shape maps onto a
   * future bennu-be classpath/effective-POM payload field-for-field.
   *
   * Reuses shared widgets (PanelShell, SidebarSection, Badge, EmptyState) + the
   * Bennu-local BennuFilterBar / BennuSketchBanner shared by the other tool panels.
   */
  import { Library, Package, GitFork } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BennuFilterBar from './BennuFilterBar.svelte';
  import BennuSketchBanner from './BennuSketchBanner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import {
    dependencyModules,
    coord,
    type Dependency,
    type DependencyModule,
  } from '$lib/stores/bennu/dependencies.svelte';

  let filter = $state('');

  // MOCK — fixed demo set until bennu-be resolves the classpath.
  const modules = $derived<DependencyModule[]>(
    projectStore.project ? dependencyModules() : [],
  );

  function matches(d: Dependency, q: string): boolean {
    if (!q) return true;
    const hay = `${coord(d)} ${d.version} ${d.scope} ${
      d.origin.kind === 'inherited' ? d.origin.from : ''
    }`.toLowerCase();
    return hay.includes(q);
  }

  // Modules with their filtered dependency lists; empty modules drop out while a
  // filter is active so the panel only shows what matched.
  const filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return modules
      .map((m) => ({ module: m.module, deps: m.dependencies.filter((d) => matches(d, q)) }))
      .filter((m) => m.deps.length > 0 || !q);
  });

  const total = $derived(filtered.reduce((n, m) => n + m.deps.length, 0));

  // Expansion set (all groups open by default; keyboard-toggleable via the section
  // header, exactly like the Maven / Structure tool groups).
  const open = $state<Record<string, boolean>>({});
  function isOpen(id: string): boolean { return open[id] ?? true; }

  function scopeTone(scope: string): 'success' | 'info' | 'warning' | 'neutral' {
    if (scope === 'compile') return 'success';
    if (scope === 'provided') return 'info';
    if (scope === 'runtime') return 'neutral';
    if (scope === 'test') return 'warning';
    return 'neutral';
  }
</script>

<PanelShell title="Dependencies" count={projectStore.project ? total : null}>
  {#snippet icon()}<Library size={13} />{/snippet}
  {#if projectStore.project}
    {#snippet toolbar()}
      <BennuFilterBar bind:query={filter} placeholder="Filter dependencies…" />
    {/snippet}
  {/if}

  {#if !projectStore.project}
    <EmptyState message="Open a project to see its dependencies." />
  {:else}
    <BennuSketchBanner text="Sketch — coordinates from the effective POM aren't wired yet." />

    <div class="dep-body">
      {#if filtered.length === 0}
        <EmptyState message={filter ? 'No dependencies match the filter.' : 'No dependencies resolved.'} compact />
      {:else}
        {#each filtered as m (m.module)}
          <SidebarSection
            label={m.module}
            expanded={isOpen(m.module)}
            onToggle={() => (open[m.module] = !isOpen(m.module))}
            badge={m.deps.length}
          >
            {#snippet icon()}<Package size={13} />{/snippet}
            <ul class="dep-list">
              {#each m.deps as d (coord(d) + '@' + d.version)}
                <li class="dep-row">
                  <div class="dep-main">
                    <span class="dep-coord mono">{coord(d)}</span>
                    <span class="dep-version mono">{d.version}</span>
                  </div>
                  <div class="dep-meta">
                    <Badge variant="tone" tone={scopeTone(d.scope)} size="sm" label={d.scope} />
                    {#if d.origin.kind === 'inherited'}
                      <span class="dep-origin" use:tooltip={`Inherited from ${d.origin.from}`}>
                        <GitFork size={10} />
                        <span class="dep-origin-txt">from {d.origin.from}</span>
                      </span>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          </SidebarSection>
        {/each}
      {/if}
    </div>
  {/if}
</PanelShell>

<style>
  /* Body rhythm mirrors the Maven tool: SidebarSection owns the group header +
     indent guideline, so the body itself keeps no horizontal padding. */
  .dep-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0 8px; }

  .dep-list {
    list-style: none; margin: 0; padding: 0 6px 2px 0;
    display: flex; flex-direction: column; gap: 1px;
  }
  .dep-row {
    display: flex; flex-direction: column; gap: 3px;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
  }
  .dep-row:hover { background: var(--bg-hover, rgba(255, 255, 255, 0.04)); }

  .dep-main {
    display: flex; align-items: baseline; gap: 8px; min-width: 0;
  }
  .dep-coord {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--text-primary);
  }
  .dep-version { flex-shrink: 0; color: var(--text-muted); }
  .mono { font-family: var(--font-code); font-size: 11px; }

  .dep-meta { display: flex; align-items: center; gap: 8px; }
  .dep-origin {
    display: inline-flex; align-items: center; gap: 3px;
    font-size: 9.5px; color: var(--text-disabled);
  }
  .dep-origin :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
  .dep-origin-txt { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
